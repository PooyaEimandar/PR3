//! Bundled GGUF backend for PR3.
//!
//! This crate is the only place that knows llama.cpp. Its public API — [`Engine::load`]
//! and [`Engine::chat`] over PR3's own [`ChatMessage`] type — is deliberately small and
//! frozen, so llama.cpp and binding upgrades stay inside this crate (Roadmap §4.3,
//! ADR-003). It currently wraps the `llama-cpp-2` bindings and needs no `unsafe` itself.

#![forbid(unsafe_code)]

mod text;

use std::num::NonZeroU32;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{LlamaChatMessage, LlamaChatTemplate, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

use crate::text::{ThinkFilter, Utf8Stream};

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
}

impl ChatRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineConfig {
    /// Context window in tokens: prompt plus answer.
    pub n_ctx: u32,
    /// Longest answer, in tokens.
    pub max_new_tokens: u32,
    /// Layers offloaded to the GPU (Metal/Vulkan); 0 means CPU only.
    pub gpu_layers: u32,
    /// Allow Qwen3's reasoning block. Off by default: on-device answers stay short and
    /// fast, and long reasoning belongs on the node (Roadmap §4.3).
    pub thinking: bool,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: i32,
    pub seed: u32,
}

impl Default for EngineConfig {
    /// Qwen3's recommended non-thinking sampling settings.
    fn default() -> Self {
        Self {
            n_ctx: 4096,
            max_new_tokens: 512,
            gpu_layers: 0,
            thinking: false,
            temperature: 0.7,
            top_p: 0.8,
            top_k: 20,
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
    template: LlamaChatTemplate,
    /// The model's template knows Qwen3-style `<think>` blocks.
    template_has_thinking: bool,
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
        let template_has_thinking = template
            .as_c_str()
            .to_str()
            .is_ok_and(|t| t.contains("<think>"));
        let label = path.file_stem().map_or_else(
            || "local model".to_string(),
            |s| s.to_string_lossy().into_owned(),
        );
        Ok(Self {
            model,
            template,
            template_has_thinking,
            config,
            label,
            generating: Mutex::new(()),
        })
    }

    /// Model name for display, from the file name (e.g. "Qwen3-0.6B-Q8_0").
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Answers the conversation. `on_text` receives visible text as it is generated;
    /// the returned string is the complete visible answer.
    pub fn chat(
        &self,
        messages: &[ChatMessage],
        on_text: &mut dyn FnMut(&str),
    ) -> Result<String, EngineError> {
        let _one_at_a_time = self
            .generating
            .lock()
            .map_err(|_| EngineError::Decode("engine is unavailable".to_string()))?;
        let backend = backend()?;
        let vocab = self.model.vocab();

        let prompt_tokens = self.prompt_tokens(messages)?;
        let n_ctx = self.config.n_ctx;
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(n_ctx))
            .with_n_batch(n_ctx);
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
            LlamaSampler::top_k(self.config.top_k),
            LlamaSampler::top_p(self.config.top_p, 1),
            LlamaSampler::temp(self.config.temperature),
            LlamaSampler::dist(self.config.seed),
        ]);

        let mut utf8 = Utf8Stream::default();
        let mut think = ThinkFilter::default();
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
        for position in first..first + budget {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);
            if vocab.is_eog(token) {
                break;
            }
            // `special = true` so `<think>` tags are visible to the filter.
            let piece = vocab.token_to_piece(token, true, None);
            emit(think.push(&utf8.push(&piece)), &mut answer);

            batch.clear();
            batch
                .add(token, pos(position)?, &[0], true)
                .map_err(|e| EngineError::Decode(e.to_string()))?;
            ctx.decode(&mut batch)
                .map_err(|e| EngineError::Decode(e.to_string()))?;
        }
        emit(think.finish(), &mut answer);
        Ok(answer.trim().to_string())
    }

    /// Builds and tokenizes the prompt, dropping the oldest messages if it would not
    /// leave room for an answer.
    fn prompt_tokens(
        &self,
        messages: &[ChatMessage],
    ) -> Result<Vec<llama_cpp_2::token::LlamaToken>, EngineError> {
        let vocab = self.model.vocab();
        let room_for_answer = self.config.max_new_tokens.min(self.config.n_ctx / 2) as usize;
        let mut start = 0;
        loop {
            let prompt = self.render(&messages[start..])?;
            let tokens = vocab.tokenize(prompt.as_bytes(), true, true);
            if tokens.len() + room_for_answer <= self.config.n_ctx as usize {
                return Ok(tokens);
            }
            if start + 1 >= messages.len() {
                return Err(EngineError::ContextFull {
                    prompt_tokens: tokens.len(),
                    n_ctx: self.config.n_ctx,
                });
            }
            start += 1;
        }
    }

    fn render(&self, messages: &[ChatMessage]) -> Result<String, EngineError> {
        let chat = messages
            .iter()
            .map(|m| LlamaChatMessage::new(m.role.as_str().to_string(), m.content.clone()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| EngineError::Template(e.to_string()))?;
        let mut prompt = self
            .model
            .apply_chat_template(&self.template, &chat, true)
            .map_err(|e| EngineError::Template(e.to_string()))?;
        // Qwen3's own template does this when `enable_thinking=False`: an empty
        // reasoning block tells the model to answer directly.
        if !self.config.thinking && self.template_has_thinking {
            prompt.push_str("<think>\n\n</think>\n\n");
        }
        Ok(prompt)
    }
}

fn pos(i: usize) -> Result<i32, EngineError> {
    i32::try_from(i).map_err(|_| EngineError::Decode("position out of range".to_string()))
}
