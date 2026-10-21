//! Bundled GGUF backend for PR3.
//!
//! This crate is the only place that knows llama.cpp and the model's prompt format. Its
//! public API — [`Engine::load`], [`Engine::chat`] and [`Engine::chat_with_tools`] over
//! PR3's own [`ChatMessage`] type — is deliberately small, so llama.cpp and binding
//! upgrades stay inside this crate (Roadmap §4.3, ADR-003). It runs the Qwen3 and
//! Qwen3.5-family (3.5, 3.6, 3.8) GGUFs, recognising which from the model's own chat
//! template. It wraps the `llama-cpp-2` bindings and needs no `unsafe` itself.

#![forbid(unsafe_code)]

mod prompt;
mod text;
mod tools;

use std::num::NonZeroU32;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::sampling::LlamaSampler;

pub use crate::prompt::PromptFormat;
use crate::text::{BlockExtractor, ThinkFilter, Utf8Stream};
use crate::tools::{CALL_CLOSE, CALL_OPEN};
pub use crate::tools::{ToolCallText, ToolDef};

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
    /// and long reasoning belongs on the node (Roadmap §4.3).
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
        }
    }
}

impl std::error::Error for EngineError {}

/// A loaded GGUF model. Generation is serialised: one answer at a time per engine.
pub struct Engine {
    model: LlamaModel,
    format: PromptFormat,
    config: EngineConfig,
    label: String,
    generating: Mutex<()>,
}

impl Engine {
    pub fn load(path: &Path, config: EngineConfig) -> Result<Self, EngineError> {
        if !path.is_file() {
            return Err(EngineError::Load(format!("{} not found", path.display())));
        }
        let backend = backend()?;
        let params = LlamaModelParams::default().with_n_gpu_layers(config.gpu_layers);
        let model = LlamaModel::load_from_file(backend, path, &params)
            .map_err(|e| EngineError::Load(e.to_string()))?;
        let template = model
            .chat_template(None)
            .map_err(|e| EngineError::Template(e.to_string()))?;
        let format = template
            .as_c_str()
            .to_str()
            .ok()
            .and_then(PromptFormat::detect)
            .ok_or_else(|| {
                EngineError::Template(
                    "unsupported model: PR3 runs Qwen3 and Qwen3.5-family GGUFs".to_string(),
                )
            })?;
        let label = path.file_stem().map_or_else(
            || "local model".to_string(),
            |s| s.to_string_lossy().into_owned(),
        );
        Ok(Self {
            model,
            format,
            config,
            label,
            generating: Mutex::new(()),
        })
    }

    /// Model name for display, from the file name (e.g. "Qwen3.5-4B-Q4_K_M").
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The prompt format read from the model's chat template.
    pub fn format(&self) -> PromptFormat {
        self.format
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
        let _one_at_a_time = self
            .generating
            .lock()
            .map_err(|_| EngineError::Decode("engine is unavailable".to_string()))?;
        let backend = backend()?;
        let vocab = self.model.vocab();

        let prompt_tokens = self.prompt_tokens(messages, tools)?;
        let n_ctx = self.config.n_ctx;
        let mut ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(n_ctx))
            .with_n_batch(n_ctx);
        if self.config.gpu_layers == 0 {
            // Otherwise llama.cpp still offloads the KV cache and large operations to
            // the GPU, which is wrong where the GPU is unusable (iOS simulator).
            ctx_params = ctx_params.with_offload_kqv(false).with_op_offload(false);
        }
        let mut ctx = self
            .model
            .new_context(backend, ctx_params)
            .map_err(|e| EngineError::Decode(e.to_string()))?;

        let mut batch = LlamaBatch::new(n_ctx as usize, 1);
        let last = prompt_tokens.len().saturating_sub(1);
        for (i, token) in prompt_tokens.iter().enumerate() {
            batch
                .add(*token, pos(i)?, &[0], i == last)
                .map_err(|e| EngineError::Decode(e.to_string()))?;
        }
        ctx.decode(&mut batch)
            .map_err(|e| EngineError::Decode(e.to_string()))?;

        let mut sampler = LlamaSampler::chain_simple([
            // Over the answer so far, as vLLM applies Qwen's recommended penalty.
            LlamaSampler::penalties(
                self.model.n_vocab(),
                64,
                1.0,
                0.0,
                self.config.presence_penalty,
            ),
            LlamaSampler::top_k(self.config.top_k),
            LlamaSampler::top_p(self.config.top_p, 1),
            LlamaSampler::temp(self.config.temperature),
            LlamaSampler::dist(self.config.seed),
        ]);

        // Model bytes → UTF-8 → hide a leading <think> block → pull out <tool_call>s.
        let mut utf8 = Utf8Stream::default();
        let mut think = if self.format.opens_think(self.config.thinking) {
            ThinkFilter::inside_think()
        } else {
            ThinkFilter::default()
        };
        let mut calls = BlockExtractor::new(CALL_OPEN, CALL_CLOSE);
        let mut answer = String::new();
        let mut emit = |visible: String, answer: &mut String| {
            if !visible.is_empty() {
                on_text(&visible);
                answer.push_str(&visible);
            }
        };

        let budget = (n_ctx as usize)
            .saturating_sub(prompt_tokens.len())
            .min(self.config.max_new_tokens as usize);
        let first = prompt_tokens.len();
        let mut completion_tokens = 0u32;
        for position in first..first + budget {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);
            if vocab.is_eog(token) {
                break;
            }
            completion_tokens += 1;
            // `special = true` so `<think>` and `<tool_call>` tags reach the filters.
            let piece = vocab.token_to_piece(token, true, None);
            emit(calls.push(&think.push(&utf8.push(&piece))), &mut answer);

            batch.clear();
            batch
                .add(token, pos(position)?, &[0], true)
                .map_err(|e| EngineError::Decode(e.to_string()))?;
            ctx.decode(&mut batch)
                .map_err(|e| EngineError::Decode(e.to_string()))?;
        }
        let tail = calls.push(&think.finish());
        emit(tail, &mut answer);
        emit(calls.finish(), &mut answer);

        Ok(ChatOutput {
            text: answer.trim().to_string(),
            tool_calls: calls
                .blocks()
                .iter()
                .map(|b| self.format.parse_call(b, tools))
                .collect(),
            prompt_tokens: u32::try_from(prompt_tokens.len()).unwrap_or(u32::MAX),
            completion_tokens,
        })
    }

    /// Builds and tokenizes the prompt. If it would not leave room for an answer, the
    /// oldest messages are dropped — never the system prompt, and never so that the
    /// window starts with an orphaned tool result.
    fn prompt_tokens(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
    ) -> Result<Vec<llama_cpp_2::token::LlamaToken>, EngineError> {
        let vocab = self.model.vocab();
        let room_for_answer = self.config.max_new_tokens.min(self.config.n_ctx / 2) as usize;
        let (system, rest) = match messages.split_first() {
            Some((first, rest)) if first.role == ChatRole::System => (Some(first), rest),
            _ => (None, messages),
        };
        let mut start = 0;
        loop {
            while rest.get(start).is_some_and(|m| m.role == ChatRole::Tool) {
                start += 1;
            }
            let window: Vec<&ChatMessage> = system
                .into_iter()
                .chain(rest.get(start..).unwrap_or_default())
                .collect();
            let prompt = prompt::render(self.format, &window, tools, self.config.thinking);
            let tokens = vocab.tokenize(prompt.as_bytes(), true, true);
            if tokens.len() + room_for_answer <= self.config.n_ctx as usize {
                return Ok(tokens);
            }
            if start + 1 >= rest.len() {
                return Err(EngineError::ContextFull {
                    prompt_tokens: tokens.len(),
                    n_ctx: self.config.n_ctx,
                });
            }
            start += 1;
        }
    }
}

fn pos(i: usize) -> Result<i32, EngineError> {
    i32::try_from(i).map_err(|_| EngineError::Decode("position out of range".to_string()))
}
