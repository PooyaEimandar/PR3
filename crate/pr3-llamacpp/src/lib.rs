//! Bundled GGUF backend for PR3.
//!
//! This crate is the only place that knows llama.cpp and the model's prompt format. Its
//! public API — [`Engine::load`], [`Engine::chat`] and [`Engine::chat_with_tools`] over
//! PR3's own [`ChatMessage`] type — is deliberately small, so llama.cpp and binding
//! upgrades stay inside this crate (docs Sec. 4.3, ADR-003). It runs the Qwen3 and
//! Qwen3.5-family (3.5, 3.6, 3.8) GGUFs, recognising which from the model's own chat
//! template. It wraps the `llama-cpp-2` bindings and needs no `unsafe` itself.
//!
//! Each engine keeps one llama.cpp context on its own thread, so a call only processes
//! the part of its prompt that earlier calls have not (see `cache`).

#![forbid(unsafe_code)]

mod cache;
mod prompt;
mod session;
mod text;
mod tool;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;

use llama_cpp_2::llama_backend::LlamaBackend;

pub use crate::prompt::PromptFormat;

/// For the fuzz targets in `fuzz/` only, which build with `--cfg fuzzing`: the output
/// path from token bytes to tool calls, exactly as a session runs it, without a model.
#[cfg(fuzzing)]
#[doc(hidden)]
pub mod fuzzing {
    use crate::text::{BlockExtractor, ThinkFilter, Utf8Stream};
    use crate::tool::{CALL_CLOSE, CALL_OPEN};
    use crate::{PromptFormat, ToolCallText, ToolDef};

    /// Streams `pieces` as token bytes through UTF-8 reassembly, `<think>` filtering
    /// and tool-call extraction, then parses every call in `format`. Returns the
    /// visible answer and the calls.
    pub fn parse_output(
        pieces: &[Vec<u8>],
        format: PromptFormat,
        thinking: bool,
        tools: &[ToolDef],
    ) -> (String, Vec<ToolCallText>) {
        let mut utf8 = Utf8Stream::default();
        let mut think = if format.opens_think(thinking) {
            ThinkFilter::inside_think()
        } else {
            ThinkFilter::default()
        };
        let mut calls = BlockExtractor::new(CALL_OPEN, CALL_CLOSE);
        let mut answer = String::new();
        for piece in pieces {
            answer.push_str(&calls.push(&think.push(&utf8.push(piece))));
        }
        let flushed = think.push(&utf8.finish());
        answer.push_str(&calls.push(&flushed));
        let tail = think.finish();
        answer.push_str(&calls.push(&tail));
        answer.push_str(&calls.finish());
        let parsed = calls
            .blocks()
            .iter()
            .map(|b| format.parse_call(b, tools))
            .collect();
        (answer, parsed)
    }
}
use crate::session::{Event, Request};
pub use crate::tool::{ToolCallText, ToolDef};

/// llama.cpp's backend may be initialised once per process.
static BACKEND: OnceLock<Result<LlamaBackend, String>> = OnceLock::new();

fn backend() -> Result<&'static LlamaBackend, EngineError> {
    BACKEND
        .get_or_init(|| {
            // llama.cpp logs heavily to stderr; keep it quiet unless asked.
            let verbose = std::env::var_os("PR3_LLAMA_LOGS").is_some();
            llama_cpp_2::send_logs_to_tracing(
                llama_cpp_2::LogOptions::default().with_logs_enabled(verbose),
            );
            LlamaBackend::init().map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| EngineError::Backend(e.clone()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    System,
    User,
    Assistant,
    /// A tool result, written into the prompt as a `<tool_response>` block.
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    /// The calls an assistant turn made; the engine writes them in the model's format.
    pub tool_calls: Vec<ToolCallText>,
}

impl ChatMessage {
    pub fn new(role: ChatRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_calls: Vec::new(),
        }
    }
}

/// One model turn: the visible answer plus any tool calls it made.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatOutput {
    pub text: String,
    /// As written by the model; the caller validates them.
    pub tool_calls: Vec<ToolCallText>,
    pub prompt_tokens: u32,
    /// Prompt tokens an earlier call had already processed, so this one skipped them.
    pub reused_tokens: u32,
    pub completion_tokens: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineConfig {
    /// Context window in tokens: prompt plus answer.
    pub n_ctx: u32,
    /// Longest answer, in tokens.
    pub max_new_tokens: u32,
    /// Layers offloaded to the GPU (Metal/Vulkan). 0 means fully CPU: no layers, KV
    /// cache or individual operations go to the GPU.
    pub gpu_layers: u32,
    /// Allow the model's reasoning block. Off by default: answers stay short and fast,
    /// and long reasoning belongs on the node (docs Sec. 4.3).
    pub thinking: bool,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: i32,
    /// Penalty for repeating a token already in the answer; 0 disables it.
    pub presence_penalty: f32,
    pub seed: u32,
}

impl Default for EngineConfig {
    /// The non-thinking sampling settings Qwen's model cards recommend.
    fn default() -> Self {
        Self {
            n_ctx: 4096,
            max_new_tokens: 512,
            gpu_layers: 0,
            thinking: false,
            temperature: 0.7,
            top_p: 0.8,
            top_k: 20,
            presence_penalty: 1.5,
            seed: 0,
        }
    }
}

#[derive(Debug)]
pub enum EngineError {
    Backend(String),
    Load(String),
    Template(String),
    /// The conversation does not fit the context window even with only the last message.
    ContextFull {
        prompt_tokens: usize,
        n_ctx: u32,
    },
    Decode(String),
    /// The engine was stopped, for example because the app is quitting.
    Stopped,
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Backend(e) => write!(f, "llama.cpp backend: {e}"),
            Self::Load(e) => write!(f, "could not load model: {e}"),
            Self::Template(e) => write!(f, "chat template: {e}"),
            Self::ContextFull {
                prompt_tokens,
                n_ctx,
            } => write!(
                f,
                "message too long for the model ({prompt_tokens} tokens; context is {n_ctx})"
            ),
            Self::Decode(e) => write!(f, "model failed while generating: {e}"),
            Self::Stopped => write!(f, "the model was stopped"),
        }
    }
}

impl std::error::Error for EngineError {}

/// A loaded GGUF model. Calls are answered one at a time, in order.
pub struct Engine {
    label: String,
    format: PromptFormat,
    requests: Option<mpsc::Sender<Request>>,
    thread: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}

impl Engine {
    /// Loads the model on the engine's own thread and waits until it is ready.
    pub fn load(path: &Path, config: EngineConfig) -> Result<Self, EngineError> {
        if !path.is_file() {
            return Err(EngineError::Load(format!("{} not found", path.display())));
        }
        let label = path.file_stem().map_or_else(
            || "local model".to_string(),
            |s| s.to_string_lossy().into_owned(),
        );
        let (ready, loaded) = mpsc::channel();
        let (requests, inbox) = mpsc::channel();
        let path = path.to_path_buf();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("pr3-llamacpp".to_string())
            .spawn(move || session::run(&path, config, &ready, &inbox, &stop_flag))
            .map_err(|e| EngineError::Backend(e.to_string()))?;
        match loaded.recv() {
            Ok(Ok(format)) => Ok(Self {
                label,
                format,
                requests: Some(requests),
                thread: Some(thread),
                stop,
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                let _ = thread.join();
                Err(EngineError::Load("the model thread stopped".to_string()))
            }
        }
    }

    /// Model name for display, from the file name (e.g. "Qwen3.5-4B-Q4_K_M").
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The prompt format read from the model's chat template.
    pub fn format(&self) -> PromptFormat {
        self.format
    }

    /// Ends the answer in progress at its next token, and makes every later call
    /// return [`EngineError::Stopped`]: for shutting down without waiting out a long
    /// generation.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Answers the conversation. `on_text` receives visible text as it is generated;
    /// the returned string is the complete visible answer.
    pub fn chat(
        &self,
        messages: &[ChatMessage],
        on_text: &mut dyn FnMut(&str),
    ) -> Result<String, EngineError> {
        self.chat_with_tools(messages, &[], on_text)
            .map(|out| out.text)
    }

    /// Like [`Engine::chat`], offering `tools`. Tool-call blocks are hidden from
    /// `on_text` and returned, unvalidated, in [`ChatOutput::tool_calls`].
    pub fn chat_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
        on_text: &mut dyn FnMut(&str),
    ) -> Result<ChatOutput, EngineError> {
        self.chat_with_tools_until(messages, tools, &mut |text| {
            on_text(text);
            true
        })
    }

    /// Like [`Engine::chat_with_tools`], but `on_text` decides whether to go on:
    /// returning `false` ends this answer at once with [`EngineError::Stopped`] and
    /// leaves the engine ready for the next call. For cancelling one request — a
    /// paired phone's, say — without stopping the engine.
    pub fn chat_with_tools_until(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
        on_text: &mut dyn FnMut(&str) -> bool,
    ) -> Result<ChatOutput, EngineError> {
        let stopped = || EngineError::Decode("the model thread stopped".to_string());
        let (events, received) = mpsc::channel();
        let request = Request {
            messages: messages.to_vec(),
            tools: tools.to_vec(),
            events,
        };
        self.requests
            .as_ref()
            .ok_or_else(stopped)?
            .send(request)
            .map_err(|_| stopped())?;
        for event in received {
            match event {
                // Dropping `received` makes the engine's next send fail, which ends the
                // answer at that token; the context stays consistent.
                Event::Text(text) if !on_text(&text) => return Err(EngineError::Stopped),
                Event::Text(_) => {}
                Event::Done(result) => return result,
            }
        }
        Err(stopped())
    }
}

impl Drop for Engine {
    /// Closing the queue ends the thread, which frees the context and then the model
    /// before this returns: llama.cpp's Metal backend aborts at process exit if model
    /// buffers are still alive.
    fn drop(&mut self) {
        self.stop();
        drop(self.requests.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
