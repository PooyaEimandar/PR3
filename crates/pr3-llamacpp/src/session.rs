//! The engine's own thread. It owns the model and one llama.cpp context for the engine's
//! whole life, so each call only processes the part of its prompt the context has not
//! already seen (see [`crate::cache`]). The context borrows the model, which is why both
//! live on this thread rather than in [`crate::Engine`]; when the engine is dropped the
//! thread ends and frees the context, then the model.

use std::num::NonZeroU32;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender};

use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::session::{LlamaStateSeqFlags, SeqState};
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;

use crate::cache::{Cache, Rewind};
use crate::text::{BlockExtractor, ThinkFilter, Utf8Stream};
use crate::tools::{CALL_CLOSE, CALL_OPEN};
use crate::{
    ChatMessage, ChatOutput, ChatRole, EngineConfig, EngineError, PromptFormat, ToolDef, backend,
    prompt,
};

/// One call, sent from [`crate::Engine::chat_with_tools`].
pub(crate) struct Request {
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDef>,
    pub events: Sender<Event>,
}

pub(crate) enum Event {
    Text(String),
    Done(Result<ChatOutput, EngineError>),
}

/// The thread's body: loads the model, reports the prompt format (or the error) on
/// `ready`, then answers requests until the engine closes the queue.
pub(crate) fn run(
    path: &Path,
    config: EngineConfig,
    ready: &Sender<Result<PromptFormat, EngineError>>,
    requests: &Receiver<Request>,
) {
    let model = match load(path, &config) {
        Ok(model) => model,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let mut session = match Session::new(&model, config) {
        Ok(session) => session,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    if ready.send(Ok(session.format)).is_err() {
        return;
    }
    for request in requests {
        let events = request.events;
        let result = session.answer(&request.messages, &request.tools, &mut |text| {
            let _ = events.send(Event::Text(text.to_string()));
        });
        if result.is_err() {
            // The context may be part-way through a batch; start clean next time.
            session.reset();
        }
        let _ = events.send(Event::Done(result));
    }
}

fn load(path: &Path, config: &EngineConfig) -> Result<LlamaModel, EngineError> {
    let params = LlamaModelParams::default().with_n_gpu_layers(config.gpu_layers);
    LlamaModel::load_from_file(backend()?, path, &params)
        .map_err(|e| EngineError::Load(e.to_string()))
}

struct Session<'m> {
    model: &'m LlamaModel,
    ctx: LlamaContext<'m>,
    format: PromptFormat,
    config: EngineConfig,
    cache: Cache<LlamaToken, SeqState>,
    /// Hybrid and recurrent models cannot cut their state back to an arbitrary
    /// position, so they rewind to checkpoints instead.
    needs_checkpoints: bool,
}

/// A tokenized prompt and the positions to checkpoint at while processing it.
struct Prompt {
    tokens: Vec<LlamaToken>,
    checkpoints: Vec<usize>,
}

impl<'m> Session<'m> {
    fn new(model: &'m LlamaModel, config: EngineConfig) -> Result<Self, EngineError> {
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

        let mut params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(config.n_ctx))
            .with_n_batch(config.n_ctx);
        if config.gpu_layers == 0 {
            // Otherwise llama.cpp still offloads the KV cache and large operations to
            // the GPU, which is wrong where the GPU is unusable (iOS simulator).
            params = params.with_offload_kqv(false).with_op_offload(false);
        }
        let ctx = model
            .new_context(backend()?, params)
            .map_err(|e| EngineError::Load(e.to_string()))?;

        Ok(Self {
            model,
            ctx,
            format,
            config,
            cache: Cache::new(),
            needs_checkpoints: model.is_recurrent() || model.is_hybrid(),
        })
    }

    fn reset(&mut self) {
        self.ctx.clear_kv_cache();
        self.cache.clear();
    }

    /// Brings the context in line with `prompt`, keeping as much of it as still
    /// matches. Returns how many prompt tokens are already processed.
    fn reuse(&mut self, prompt: &[LlamaToken]) -> usize {
        let kept = match self.cache.plan(prompt, !self.needs_checkpoints) {
            Rewind::None => Some(self.cache.tokens.len()),
            Rewind::Trim { to } => self.cut(to).then_some(to),
            Rewind::Restore { index } => {
                let restored = match self.cache.checkpoints.get(index) {
                    Some(c) => self.ctx.state_seq_set(&c.state, 0).is_ok().then_some(c.pos),
                    None => None,
                };
                // The restored state already ends at the checkpoint; this trims the
                // attention cache to match.
                restored.filter(|&pos| self.cut(pos))
            }
            Rewind::Clear => None,
        };
        match kept {
            Some(pos) => {
                self.cache.truncate(pos);
                pos
            }
            None => {
                self.reset();
                0
            }
        }
    }

    /// Drops everything the context holds from position `to` on.
    fn cut(&mut self, to: usize) -> bool {
        u32::try_from(to).is_ok_and(|to| {
            self.ctx
                .clear_kv_cache_seq(Some(0), Some(to), None)
                .unwrap_or(false)
        })
    }

    /// Processes `tokens` as positions `from..`, with logits for the last one only if
    /// `logits_last`.
    fn decode(
        &mut self,
        batch: &mut LlamaBatch,
        tokens: &[LlamaToken],
        from: usize,
        logits_last: bool,
    ) -> Result<(), EngineError> {
        batch.clear();
        let last = tokens.len().saturating_sub(1);
        for (i, token) in tokens.iter().enumerate() {
            batch
                .add(*token, pos(from + i)?, &[0], logits_last && i == last)
                .map_err(|e| EngineError::Decode(e.to_string()))?;
        }
        self.ctx
            .decode(batch)
            .map_err(|e| EngineError::Decode(e.to_string()))?;
        self.cache.tokens.extend_from_slice(tokens);
        Ok(())
    }

    fn answer(
        &mut self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
        on_text: &mut dyn FnMut(&str),
    ) -> Result<ChatOutput, EngineError> {
        let model = self.model;
        let vocab = model.vocab();
        let Prompt {
            tokens: prompt,
            checkpoints,
        } = self.prompt_tokens(messages, tools)?;
        let n_ctx = self.config.n_ctx as usize;
        let mut batch = LlamaBatch::new(n_ctx, 1);

        // Process only what the context has not seen, checkpointing on the way.
        let reused = self.reuse(&prompt);
        let marks = checkpoints.into_iter().filter(|&p| p > reused);
        let mut start = reused;
        for end in marks.chain(std::iter::once(prompt.len())) {
            let segment = prompt.get(start..end).unwrap_or_default();
            self.decode(&mut batch, segment, start, end == prompt.len())?;
            if end < prompt.len() {
                let state = self
                    .ctx
                    .state_seq_get(0, LlamaStateSeqFlags::PARTIAL_ONLY)
                    .map_err(|e| EngineError::Decode(e.to_string()))?;
                self.cache.add_checkpoint(state);
            }
            start = end;
        }

        let mut sampler = LlamaSampler::chain_simple([
            // Over the answer so far, as vLLM applies Qwen's recommended penalty.
            LlamaSampler::penalties(model.n_vocab(), 64, 1.0, 0.0, self.config.presence_penalty),
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

        let budget = n_ctx
            .saturating_sub(prompt.len())
            .min(self.config.max_new_tokens as usize);
        let mut completion_tokens = 0u32;
        for _ in 0..budget {
            let token = sampler.sample(&self.ctx, batch.n_tokens() - 1);
            sampler.accept(token);
            if vocab.is_eog(token) {
                break;
            }
            completion_tokens += 1;
            // `special = true` so `<think>` and `<tool_call>` tags reach the filters.
            let piece = vocab.token_to_piece(token, true, None);
            emit(calls.push(&think.push(&utf8.push(&piece))), &mut answer);
            let position = self.cache.tokens.len();
            self.decode(&mut batch, &[token], position, true)?;
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
            prompt_tokens: count(prompt.len()),
            reused_tokens: count(reused),
            completion_tokens,
        })
    }

    /// Token positions to checkpoint at, for models that need checkpoints: after the
    /// system turn, and after the last question if the template rewrites the history
    /// that follows it. A boundary counts only if the prompt's tokens split there.
    fn checkpoints(&self, rendered: &prompt::Rendered, tokens: &[LlamaToken]) -> Vec<usize> {
        if !self.needs_checkpoints {
            return Vec::new();
        }
        let query_end = rendered
            .query_end
            .filter(|_| self.format.rewrites_history());
        let vocab = self.model.vocab();
        let mut positions: Vec<usize> = [rendered.system_end, query_end]
            .into_iter()
            .flatten()
            .filter_map(|end| {
                let head = vocab.tokenize(rendered.text.get(..end)?.as_bytes(), true, true);
                tokens.starts_with(&head).then_some(head.len())
            })
            .collect();
        positions.dedup();
        positions
    }

    /// Builds and tokenizes the prompt. If it would not leave room for an answer, the
    /// oldest messages are dropped — never the system prompt, and never so that the
    /// window starts with an orphaned tool result.
    fn prompt_tokens(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
    ) -> Result<Prompt, EngineError> {
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
            let rendered = prompt::render(self.format, &window, tools, self.config.thinking);
            let tokens = vocab.tokenize(rendered.text.as_bytes(), true, true);
            if tokens.len() + room_for_answer <= self.config.n_ctx as usize {
                let checkpoints = self.checkpoints(&rendered, &tokens);
                return Ok(Prompt {
                    tokens,
                    checkpoints,
                });
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

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
