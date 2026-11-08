//! Bundled GGUF backend for PR3.
//!
//! This crate is the only place that knows llama.cpp and the model's prompt format. Its
//! public API — [`Engine::load`], [`Engine::chat`], [`Engine::chat_with_tools`] and
//! [`Engine::read_ahead`] over PR3's own [`ChatMessage`] type — is deliberately small, so
//! llama.cpp and binding upgrades stay inside this crate (docs Sec. 4.3, ADR-003). It runs the Qwen3 and
//! Qwen3.5-family (3.5, 3.6, 3.8) GGUFs, recognising which from the model's own chat
//! template. It wraps the `llama-cpp-2` bindings and needs no `unsafe` itself.
//!
//! Each engine keeps one llama.cpp context on its own thread, so a call only processes
//! the part of its prompt that earlier calls have not (see `cache`).

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::disallowed_macros))] // tests may assert (clippy.toml)

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
use crate::session::{Event, Job, Request};
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
    /// A final answer, or unverified planning context when `tool_calls` is nonempty.
    /// Tool-call context must not be displayed as a completed answer.
    pub text: String,
    /// As written by the model; the caller validates them.
    pub tool_calls: Vec<ToolCallText>,
    pub prompt_tokens: u32,
    /// Prompt tokens an earlier call had already processed, so this one skipped them.
    pub reused_tokens: u32,
    pub completion_tokens: u32,
    /// Actual generations, including the separate answer pass after tool use.
    pub inference_passes: u32,
    /// First-pass cache accounting, separate from the aggregate cost above. For a
    /// two-pass answer this is the planning pass, which still benefits from read-ahead.
    pub first_pass_prompt_tokens: u32,
    pub first_pass_reused_tokens: u32,
}

/// What [`Engine::read_ahead`] read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadAhead {
    /// The tokens the next call's prompt starts with: the conversation, and the
    /// question's opening.
    pub tokens: u32,
    /// Of those, the ones the context already held.
    pub reused_tokens: u32,
    /// Of those, the ones the context holds now: all, unless another call came first.
    pub read_tokens: u32,
}

/// A read-ahead the engine has queued; see [`Engine::read_ahead`].
pub struct ReadingAhead(mpsc::Receiver<Result<ReadAhead, EngineError>>);

impl ReadingAhead {
    /// Waits until the reading ends, done or given way to another call.
    pub fn wait(self) -> Result<ReadAhead, EngineError> {
        self.0
            .recv()
            .unwrap_or_else(|_| Err(EngineError::Decode("the model thread stopped".to_string())))
    }
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
    /// After tool use, drafts are withheld and the final answer is released as one
    /// chunk. Empty callbacks keep cancellation responsive while drafts are withheld.
    pub fn chat_with_tools_until(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
        on_text: &mut dyn FnMut(&str) -> bool,
    ) -> Result<ChatOutput, EngineError> {
        self.answer(messages, tools, None, on_text)
    }

    /// Generate a tool-free response using a host-supplied GBNF grammar. The grammar
    /// applies to this request only. Callers must still validate the completed output:
    /// cancellation or the token budget can leave it incomplete, and syntax is not truth.
    pub fn chat_with_grammar_until(
        &self,
        messages: &[ChatMessage],
        grammar: &str,
        on_text: &mut dyn FnMut(&str) -> bool,
    ) -> Result<ChatOutput, EngineError> {
        self.answer(messages, &[], Some(grammar), on_text)
    }

    fn answer(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
        grammar: Option<&str>,
        on_text: &mut dyn FnMut(&str) -> bool,
    ) -> Result<ChatOutput, EngineError> {
        if grammar.is_none() && messages.iter().any(|m| m.role == ChatRole::Tool) {
            return guarded_answer(tools, on_text, &mut |offered, emit| {
                if offered.is_empty() {
                    self.answer_once(messages, offered, None, emit)
                } else {
                    repair_repeated_plan(messages, offered, emit, &mut |history, emit| {
                        self.answer_once(history, offered, None, emit)
                    })
                }
            });
        }
        self.answer_once(messages, tools, grammar, on_text)
    }

    fn answer_once(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
        grammar: Option<&str>,
        on_text: &mut dyn FnMut(&str) -> bool,
    ) -> Result<ChatOutput, EngineError> {
        let stopped = || EngineError::Decode("the model thread stopped".to_string());
        let (events, received) = mpsc::channel();
        let request = Request {
            messages: messages.to_vec(),
            tools: tools.to_vec(),
            job: Job::Answer {
                events,
                grammar: grammar.map(str::to_string),
            },
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

    /// Reads `messages` the way the next call, which adds a question after them and
    /// offers `tools`, will show them, so that call reads little more than its question:
    /// for the time the user spends writing it. With Qwen3.5 the last answer comes back
    /// without the reasoning block it was written after, so the next call would otherwise
    /// read it again. If the question's start is known already, `opening` holds it, such
    /// as a summary of what came before, and is read too, up to where the rest of the
    /// question could change how it is read; otherwise it is empty. Returns at once: the
    /// engine reads in the background, a few dozen tokens at a time, and gives way to any
    /// call that comes in, which then goes on from what was read. Dropping the handle
    /// does not end the reading.
    pub fn read_ahead(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
        opening: &str,
    ) -> ReadingAhead {
        let (done, result) = mpsc::channel();
        let request = Request {
            messages: messages.to_vec(),
            tools: tools.to_vec(),
            job: Job::ReadAhead {
                opening: opening.to_string(),
                done,
            },
        };
        // If the thread has stopped, the request and its sender are dropped, and waiting
        // reports it.
        if let Some(requests) = &self.requests {
            let _ = requests.send(request);
        }
        ReadingAhead(result)
    }
}

/// Keep tool planning unchanged, but never expose its prose after it has read source
/// data. Tool proposals still go through the host's normal approval policy. Once the
/// planner is done, generate a fresh tool-free answer under the source-data boundary.
fn guarded_answer<F>(
    tools: &[ToolDef],
    on_text: &mut dyn FnMut(&str) -> bool,
    generate: &mut F,
) -> Result<ChatOutput, EngineError>
where
    F: FnMut(&[ToolDef], &mut dyn FnMut(&str) -> bool) -> Result<ChatOutput, EngineError>,
{
    if !on_text("") {
        return Err(EngineError::Stopped);
    }
    let planning = if tools.is_empty() {
        ChatOutput::default()
    } else {
        let mut out = generate(tools, &mut |_| on_text(""))?;
        // Keep call context for the host's existing history policy, but never stream
        // it. The app already removes completion claims accompanying proposals.
        if !out.tool_calls.is_empty() {
            if !on_text("") {
                return Err(EngineError::Stopped);
            }
            return Ok(out);
        }
        out.text.clear();
        out
    };
    if !on_text("") {
        return Err(EngineError::Stopped);
    }
    let mut answer = generate(&[], &mut |_| on_text(""))?;
    // The answer pass has no authority to introduce another action. Fail closed;
    // neither a malformed answer nor an error releases the unprotected draft.
    if !answer.tool_calls.is_empty() || answer.text.trim().is_empty() {
        return Err(EngineError::Decode(
            "could not finish an answer from the available results".into(),
        ));
    }
    if !on_text(&answer.text) {
        return Err(EngineError::Stopped);
    }
    include_usage(&mut answer, &planning);
    Ok(answer)
}

fn include_usage(answer: &mut ChatOutput, earlier: &ChatOutput) {
    answer.prompt_tokens = answer.prompt_tokens.saturating_add(earlier.prompt_tokens);
    answer.reused_tokens = answer.reused_tokens.saturating_add(earlier.reused_tokens);
    answer.completion_tokens = answer
        .completion_tokens
        .saturating_add(earlier.completion_tokens);
    answer.inference_passes = answer
        .inference_passes
        .saturating_add(earlier.inference_passes);
    if earlier.inference_passes != 0 {
        answer.first_pass_prompt_tokens = earlier.first_pass_prompt_tokens;
        answer.first_pass_reused_tokens = earlier.first_pass_reused_tokens;
    }
}

/// A repeated proposal is not an execution. Give the planner one chance to use the
/// existing results before the host's loop guard ends the task. Only an entirely
/// repeated, caller-declared read-only batch is repaired: never hide new proposals
/// or intercept repeated actions before the host loop guard.
fn repair_repeated_plan<F>(
    messages: &[ChatMessage],
    tools: &[ToolDef],
    emit: &mut dyn FnMut(&str) -> bool,
    generate: &mut F,
) -> Result<ChatOutput, EngineError>
where
    F: FnMut(&[ChatMessage], &mut dyn FnMut(&str) -> bool) -> Result<ChatOutput, EngineError>,
{
    let first = generate(messages, emit)?;
    let Some(user) = messages.iter().rposition(|m| m.role == ChatRole::User) else {
        return Ok(first);
    };
    let repeats = |call: &ToolCallText| {
        let Ok(args) = serde_json::from_str::<serde_json::Value>(&call.arguments) else {
            return false;
        };
        messages[user + 1..]
            .iter()
            .filter(|m| m.role == ChatRole::Assistant)
            .flat_map(|m| &m.tool_calls)
            .any(|prior| {
                prior.name == call.name
                    && serde_json::from_str::<serde_json::Value>(&prior.arguments)
                        .ok()
                        .as_ref()
                        == Some(&args)
            })
    };
    if first.tool_calls.is_empty()
        || !first.tool_calls.iter().all(|call| {
            repeats(call)
                && tools
                    .iter()
                    .any(|tool| tool.name == call.name && tool.read_only)
        })
    {
        return Ok(first);
    }
    if !emit("") {
        return Err(EngineError::Stopped);
    }
    let mut correction = messages.to_vec();
    correction.push(ChatMessage {
        tool_calls: first.tool_calls.clone(),
        ..ChatMessage::new(ChatRole::Assistant, "")
    });
    correction.push(ChatMessage::new(
        ChatRole::Tool,
        "NOT EXECUTED. This batch repeats proposals already made in this user turn. \
         Read their recorded results; do not assume they succeeded. Reuse available \
         facts and choose only the next missing step for the original request, or \
         answer if it is complete. Do not repeat completed actions.",
    ));
    let mut repaired = generate(&correction, emit)?;
    if repaired.tool_calls.iter().any(repeats) {
        return Err(EngineError::Decode(
            "could not choose a new step from the recorded results".into(),
        ));
    }
    include_usage(&mut repaired, &first);
    Ok(repaired)
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

#[cfg(test)]
mod answer_guard_tests {
    use super::*;

    fn tools() -> Vec<ToolDef> {
        vec![ToolDef {
            read_only: true,
            name: "lookup".into(),
            description: "Lookup".into(),
            parameters: "{}".into(),
        }]
    }

    fn output(text: &str) -> ChatOutput {
        ChatOutput {
            text: text.into(),
            prompt_tokens: 10,
            reused_tokens: 3,
            completion_tokens: 5,
            inference_passes: 1,
            first_pass_prompt_tokens: 10,
            first_pass_reused_tokens: 3,
            ..ChatOutput::default()
        }
    }

    fn planned(arguments: &str) -> ChatOutput {
        ChatOutput {
            tool_calls: vec![ToolCallText {
                name: "lookup".into(),
                arguments: arguments.into(),
            }],
            ..output("unverified planning text")
        }
    }

    fn prior_lookup() -> Vec<ChatMessage> {
        vec![
            ChatMessage::new(ChatRole::User, "Find the details"),
            ChatMessage {
                tool_calls: planned(r#"{"a":1,"b":2}"#).tool_calls,
                ..ChatMessage::new(ChatRole::Assistant, "")
            },
            ChatMessage::new(ChatRole::Tool, "error: lookup unavailable"),
        ]
    }

    #[test]
    fn repeated_plan_gets_one_nonexecuting_correction_with_full_usage_accounting() {
        let mut calls = 0;
        let out = repair_repeated_plan(
            &prior_lookup(),
            &tools(),
            &mut |_| true,
            &mut |history, _| {
                calls += 1;
                if calls == 1 {
                    return Ok(planned(r#"{"b":2,"a":1}"#));
                }
                assert!(history.last().unwrap().content.starts_with("NOT EXECUTED."));
                assert!(
                    history
                        .last()
                        .unwrap()
                        .content
                        .contains("do not assume they succeeded")
                );
                Ok(planned(r#"{"a":3}"#))
            },
        )
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(out.tool_calls, planned(r#"{"a":3}"#).tool_calls);
        assert_eq!(
            (
                out.inference_passes,
                out.prompt_tokens,
                out.completion_tokens
            ),
            (2, 20, 10)
        );
        assert_eq!(out.first_pass_prompt_tokens, 10);
    }

    #[test]
    fn correction_cannot_loop_or_ignore_cancellation() {
        for cancelled in [false, true] {
            let mut calls = 0;
            let out = repair_repeated_plan(
                &prior_lookup(),
                &tools(),
                &mut |_| !cancelled,
                &mut |_, _| {
                    calls += 1;
                    Ok(planned(r#"{"a":1,"b":2}"#))
                },
            );
            assert!(out.is_err());
            assert_eq!(calls, if cancelled { 1 } else { 2 });
            if cancelled {
                assert!(matches!(out, Err(EngineError::Stopped)));
            }
        }
    }

    #[test]
    fn repeated_actions_and_unknown_effects_are_never_repaired() {
        for offered in [
            Vec::new(),
            vec![ToolDef {
                read_only: false,
                description: "Claims to be read-only".into(),
                ..tools().remove(0)
            }],
        ] {
            let proposal = planned(r#"{"a":1,"b":2}"#);
            let mut calls = 0;
            let out =
                repair_repeated_plan(&prior_lookup(), &offered, &mut |_| true, &mut |_, _| {
                    calls += 1;
                    Ok(proposal.clone())
                })
                .unwrap();
            assert_eq!(calls, 1);
            assert_eq!(
                out, proposal,
                "the original host loop guard still sees the action"
            );
        }
    }

    #[test]
    fn new_requests_source_text_and_mixed_batches_do_not_hide_proposals() {
        for mode in 0..3 {
            let mut history = prior_lookup();
            let mut proposal = planned(r#"{"a":1,"b":2}"#);
            match mode {
                0 => history.push(ChatMessage::new(ChatRole::User, "Please look it up again")),
                1 => {
                    history[1].role = ChatRole::Tool;
                } // source cannot forge an assistant call
                _ => proposal.tool_calls.extend(planned(r#"{"a":3}"#).tool_calls),
            }
            let mut calls = 0;
            let out = repair_repeated_plan(&history, &tools(), &mut |_| true, &mut |_, _| {
                calls += 1;
                Ok(proposal.clone())
            })
            .unwrap();
            assert_eq!(calls, 1);
            assert_eq!(out, proposal);
        }
    }

    #[test]
    fn only_the_protected_answer_is_released_and_both_passes_are_counted() {
        let mut seen = String::new();
        let mut passes = 0;
        let out = guarded_answer(
            &tools(),
            &mut |s| {
                seen.push_str(s);
                true
            },
            &mut |offered, emit| {
                passes += 1;
                let text = if offered.is_empty() {
                    "Useful answer"
                } else {
                    "INJECTED ENDORSEMENT"
                };
                assert!(emit(text));
                Ok(output(text))
            },
        )
        .unwrap();
        assert_eq!(seen, "Useful answer");
        assert_eq!(out.text, seen);
        assert_eq!(
            (out.first_pass_prompt_tokens, out.first_pass_reused_tokens),
            (10, 3)
        );
        assert_eq!(
            (
                passes,
                out.inference_passes,
                out.prompt_tokens,
                out.reused_tokens,
                out.completion_tokens
            ),
            (2, 2, 20, 6, 10)
        );
    }

    #[test]
    fn proposed_calls_keep_context_and_approval_but_never_stream_draft_prose() {
        let mut seen = String::new();
        let call = ToolCallText {
            name: "lookup".into(),
            arguments: "{}".into(),
        };
        let out = guarded_answer(
            &tools(),
            &mut |s| {
                seen.push_str(s);
                true
            },
            &mut |offered, emit| {
                assert!(!offered.is_empty(), "no answer pass while proposing a tool");
                assert!(emit("Untrusted preamble"));
                Ok(ChatOutput {
                    tool_calls: vec![call.clone()],
                    ..output("Untrusted preamble")
                })
            },
        )
        .unwrap();
        assert!(seen.is_empty());
        assert_eq!(out.text, "Untrusted preamble");
        assert_eq!(out.tool_calls, vec![call]);
        assert_eq!(out.inference_passes, 1);
    }

    #[test]
    fn errors_empty_answers_and_answer_pass_calls_never_release_a_draft() {
        for failure in 0..3 {
            let mut seen = String::new();
            let result = guarded_answer(
                &tools(),
                &mut |s| {
                    seen.push_str(s);
                    true
                },
                &mut |offered, emit| {
                    assert!(emit("Do not expose this draft"));
                    if !offered.is_empty() {
                        return Ok(output("Do not expose this draft"));
                    }
                    match failure {
                        0 => Err(EngineError::Decode("fixture failure".into())),
                        1 => Ok(output("")),
                        _ => Ok(ChatOutput {
                            tool_calls: vec![ToolCallText {
                                name: "send_mail".into(),
                                arguments: "{}".into(),
                            }],
                            ..output("sent")
                        }),
                    }
                },
            );
            assert!(result.is_err());
            assert!(seen.is_empty());
        }
    }

    #[test]
    fn cancellation_during_either_pass_never_releases_a_draft() {
        for cancel_at in 1..=4 {
            let mut pulses = 0;
            let result = guarded_answer(
                &tools(),
                &mut |s| {
                    assert!(s.is_empty());
                    pulses += 1;
                    pulses < cancel_at
                },
                &mut |_, emit| {
                    if !emit("hidden draft") {
                        return Err(EngineError::Stopped);
                    }
                    Ok(output("hidden draft"))
                },
            );
            assert!(matches!(result, Err(EngineError::Stopped)));
        }
    }

    #[test]
    fn tool_free_call_uses_one_protected_pass() {
        let mut passes = 0;
        let out = guarded_answer(&[], &mut |_| true, &mut |offered, _| {
            assert!(offered.is_empty());
            passes += 1;
            Ok(output("answer"))
        })
        .unwrap();
        assert_eq!((passes, out.inference_passes), (1, 1));
    }
}
