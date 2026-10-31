//! The engine's own thread. It owns the model and one llama.cpp context for the engine's
//! whole life, so each call only processes the part of its prompt the context has not
//! already seen (see [`crate::cache`]). The context borrows the model, which is why both
//! live on this thread rather than in [`crate::Engine`]; when the engine is dropped the
//! thread ends and frees the context, then the model.

use std::num::NonZeroU32;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};

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
use crate::tool::{CALL_CLOSE, CALL_OPEN};
use crate::{
    ChatMessage, ChatOutput, ChatRole, EngineConfig, EngineError, PromptFormat, ReadAhead, ToolDef,
    backend, prompt,
};

/// One call, sent from [`crate::Engine::chat_with_tools`] or [`crate::Engine::read_ahead`].
pub(crate) struct Request {
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDef>,
    pub job: Job,
}

pub(crate) enum Job {
    /// Answer, streaming the visible text.
    Answer(Sender<Event>),
    /// Only read the conversation, as the prompt of the next answer, whose question
    /// begins with `opening`, will show it.
    ReadAhead {
        opening: String,
        done: Sender<Result<ReadAhead, EngineError>>,
    },
}

/// How many tokens reading ahead processes at once. Before each batch it gives way to any
/// call that has come in, so a call waits for at most one batch, which it needs anyway
/// when it continues the conversation.
const READ_AHEAD_BATCH: usize = 32;

pub(crate) enum Event {
    Text(String),
    Done(Result<ChatOutput, EngineError>),
}

/// The thread's body: loads the model, reports the prompt format (or the error) on
/// `ready`, then answers requests until the engine closes the queue. Once `stop` is set
/// every answer ends at the next token.
pub(crate) fn run(
    path: &Path,
    config: EngineConfig,
    ready: &Sender<Result<PromptFormat, EngineError>>,
    requests: &Receiver<Request>,
    stop: &AtomicBool,
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
    let mut waiting = None;
    while let Some(request) = waiting.take().or_else(|| requests.recv().ok()) {
        let Request {
            messages,
            tools,
            job,
        } = request;
        match job {
            Job::Answer(events) => {
                // A caller that stopped listening ends its answer early.
                let mut on_text = |text: &str| events.send(Event::Text(text.to_string())).is_ok();
                let result = session.answer(&messages, &tools, &mut on_text, stop);
                if matches!(result, Err(EngineError::Decode(_) | EngineError::Stopped)) {
                    // The context may be part-way through a batch; start clean next time.
                    session.reset();
                }
                let _ = events.send(Event::Done(result));
            }
            Job::ReadAhead { opening, done } => {
                // Any other call ends the reading, keeping what was read, and goes next.
                let mut go_on = || {
                    if stop.load(Ordering::Relaxed) {
                        return false;
                    }
                    match requests.try_recv() {
                        Ok(next) => {
                            waiting = Some(next);
                            false
                        }
                        Err(TryRecvError::Empty) => true,
                        Err(TryRecvError::Disconnected) => false,
                    }
                };
                let result = session.read_ahead(&messages, &tools, &opening, &mut go_on);
                // It stops only between batches, so the context is otherwise consistent.
                if matches!(result, Err(EngineError::Decode(_))) {
                    session.reset();
                }
                let _ = done.send(result);
            }
        }
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
    /// The context right after the system turn of the last call that offered tools, kept
    /// whole, so a prompt that starts with it reads only what follows, even after a call
    /// that started otherwise, such as a task's summary, which offers none.
    prefix: Option<Prefix>,
}

/// A system turn's tokens and the context's whole state after them: the attention cache
/// and any recurrent state.
struct Prefix {
    tokens: Vec<LlamaToken>,
    state: SeqState,
}

/// A tokenized prompt and the positions to checkpoint at while processing it.
struct Prompt {
    tokens: Vec<LlamaToken>,
    checkpoints: Vec<usize>,
    /// Where the system turn ends, if there is one.
    system_end: Option<usize>,
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
            prefix: None,
        })
    }

    fn reset(&mut self) {
        self.ctx.clear_kv_cache();
        self.cache.clear();
    }

    /// Brings the context in line with `prompt`, keeping as much of it as still
    /// matches. Returns how many prompt tokens are already processed.
    fn reuse(&mut self, prompt: &[LlamaToken]) -> usize {
        let plan = self.cache.plan(prompt, !self.needs_checkpoints);
        let keeps = match plan {
            Rewind::None => self.cache.tokens.len(),
            Rewind::Trim { to } => to,
            Rewind::Restore { index } => self.cache.checkpoints.get(index).map_or(0, |c| c.pos),
            Rewind::Clear => 0,
        };
        let from_prefix = self.prefix.as_ref().is_some_and(|p| {
            keeps < p.tokens.len() && prompt.len() > p.tokens.len() && prompt.starts_with(&p.tokens)
        });
        if from_prefix {
            return self.restore_prefix();
        }
        let kept = match plan {
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

    /// Puts the context back as it was right after the kept system turn. Returns how many
    /// tokens it then holds: none if that failed.
    fn restore_prefix(&mut self) -> usize {
        self.reset();
        let restored = match &self.prefix {
            Some(p) => self
                .ctx
                .state_seq_set(&p.state, 0)
                .is_ok()
                .then(|| p.tokens.clone()),
            None => None,
        };
        let Some(tokens) = restored else {
            self.reset();
            return 0;
        };
        self.cache.tokens = tokens;
        // The checkpoint a hybrid model rewinds to when a later prompt differs after it.
        if self.needs_checkpoints
            && let Ok(state) = self.ctx.state_seq_get(0, LlamaStateSeqFlags::PARTIAL_ONLY)
        {
            self.cache.add_checkpoint(state);
        }
        self.cache.tokens.len()
    }

    /// Keeps the context as it is, right after the system turn `tokens`, unless it is
    /// kept already. Only a copy, so failing to make one changes nothing.
    fn keep_prefix(&mut self, tokens: &[LlamaToken]) {
        if self.prefix.as_ref().is_some_and(|p| p.tokens == tokens) {
            return;
        }
        if let Ok(state) = self.ctx.state_seq_get(0, LlamaStateSeqFlags::empty()) {
            self.prefix = Some(Prefix {
                tokens: tokens.to_vec(),
                state,
            });
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

    /// Answers `messages`. `on_text` gets visible text as it is generated and returns
    /// false to end the answer early.
    fn answer(
        &mut self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
        on_text: &mut dyn FnMut(&str) -> bool,
        stop: &AtomicBool,
    ) -> Result<ChatOutput, EngineError> {
        let model = self.model;
        let vocab = model.vocab();
        let prompt = self.prompt_tokens(messages, tools)?;
        let n_ctx = self.config.n_ctx as usize;
        let mut batch = LlamaBatch::new(n_ctx, 1);

        // Process only what the context has not seen.
        let reused = self.reuse(&prompt.tokens);
        let to = prompt.tokens.len();
        self.fill(
            &mut batch,
            &prompt,
            reused..to,
            !tools.is_empty(),
            n_ctx,
            &mut || true,
        )?;
        let prompt = prompt.tokens;

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
        let mut emit = |visible: String, answer: &mut String| -> bool {
            if visible.is_empty() {
                return true;
            }
            answer.push_str(&visible);
            on_text(&visible)
        };

        let budget = n_ctx
            .saturating_sub(prompt.len())
            .min(self.config.max_new_tokens as usize);
        let mut completion_tokens = 0u32;
        for _ in 0..budget {
            if stop.load(Ordering::Relaxed) {
                return Err(EngineError::Stopped);
            }
            // `sample` also records the token for the repetition penalty.
            let token = sampler.sample(&self.ctx, batch.n_tokens() - 1);
            // A turn marker the model writes ends its answer; control tokens never
            // reach the text, so they cannot reappear as structure in the history.
            if vocab.is_eog(token) || vocab.is_control(token) {
                break;
            }
            completion_tokens += 1;
            // Without `special`, control tokens print as nothing; `<think>` and
            // `<tool_call>` are ordinary added tokens and still reach the filters.
            let piece = vocab.token_to_piece(token, false, None);
            let more = emit(calls.push(&think.push(&utf8.push(&piece))), &mut answer);
            let position = self.cache.tokens.len();
            self.decode(&mut batch, &[token], position, true)?;
            if !more {
                break;
            }
        }
        let flushed = think.push(&utf8.finish());
        let _ = emit(calls.push(&flushed), &mut answer);
        let tail = think.finish();
        let _ = emit(calls.push(&tail), &mut answer);
        let _ = emit(calls.finish(), &mut answer);

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

    /// Reads `messages` as the prompt of the next call, which adds a question after them
    /// that begins with `opening` and offers `tools`, will show them, so that call reads
    /// little more than the rest of its question. Goes [`READ_AHEAD_BATCH`] tokens at a
    /// time and ends early, keeping what it read, once `go_on` says so.
    fn read_ahead(
        &mut self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
        opening: &str,
        go_on: &mut dyn FnMut() -> bool,
    ) -> Result<ReadAhead, EngineError> {
        // Two such questions that go on differently share the tokens every one of them
        // will: up to where the rest of a question could change how its opening is read.
        let question = |rest: &str| {
            let mut next = messages.to_vec();
            next.push(ChatMessage::new(ChatRole::User, format!("{opening}{rest}")));
            self.prompt_tokens(&next, tools)
        };
        let prompt = question("x")?;
        let ahead = question("1")?
            .tokens
            .iter()
            .zip(&prompt.tokens)
            .take_while(|(a, b)| a == b)
            .count();
        // Against the whole prompt, so a context that already holds the conversation is
        // not cut back to read its last token again.
        let reused = self.reuse(&prompt.tokens).min(ahead);
        let mut batch = LlamaBatch::new(READ_AHEAD_BATCH, 1);
        let read = self.fill(
            &mut batch,
            &prompt,
            reused..ahead,
            !tools.is_empty(),
            READ_AHEAD_BATCH,
            go_on,
        )?;
        Ok(ReadAhead {
            tokens: count(ahead),
            reused_tokens: count(reused),
            read_tokens: count(read),
        })
    }

    /// Processes the prompt's tokens in `range`, at most `batch_size` at once,
    /// checkpointing on the way and keeping the context after a system turn it has not
    /// kept yet, if `offers_tools`: a call without tools, such as a summary, leaves the
    /// conversation's alone. Logits come only for the prompt's last token. Before each
    /// batch `go_on` may end it early; returns where it got to.
    fn fill(
        &mut self,
        batch: &mut LlamaBatch,
        prompt: &Prompt,
        range: std::ops::Range<usize>,
        offers_tools: bool,
        batch_size: usize,
        go_on: &mut dyn FnMut() -> bool,
    ) -> Result<usize, EngineError> {
        if range.is_empty() {
            return Ok(range.start);
        }
        let mut marks: Vec<usize> = prompt
            .checkpoints
            .iter()
            .copied()
            .chain(prompt.system_end)
            .collect();
        marks.sort_unstable();
        marks.dedup();
        marks.retain(|p| range.start < *p && *p < range.end);
        let mut start = range.start;
        for end in marks.into_iter().chain(std::iter::once(range.end)) {
            while start < end {
                if !go_on() {
                    return Ok(start);
                }
                let upto = end.min(start + batch_size.max(1));
                let tokens = prompt.tokens.get(start..upto).unwrap_or_default();
                self.decode(batch, tokens, start, upto == prompt.tokens.len())?;
                start = upto;
            }
            if prompt.checkpoints.contains(&end) {
                let state = self
                    .ctx
                    .state_seq_get(0, LlamaStateSeqFlags::PARTIAL_ONLY)
                    .map_err(|e| EngineError::Decode(e.to_string()))?;
                self.cache.add_checkpoint(state);
            }
            if prompt.system_end == Some(end) && offers_tools {
                self.keep_prefix(prompt.tokens.get(..end).unwrap_or_default());
            }
        }
        Ok(start)
    }

    /// Tokenizes a rendered prompt. Only the template's turn markers may become control
    /// tokens; message text is tokenized as text, so a message containing
    /// `<|im_start|>` cannot open a turn. Hybrid models also get their checkpoint
    /// positions: after the system turn and after the last question.
    fn tokenize(&self, rendered: &prompt::Rendered) -> Prompt {
        let vocab = self.model.vocab();
        let mut tokens = Vec::new();
        let mut ends = vec![0];
        for (i, piece) in rendered.pieces.iter().enumerate() {
            tokens.extend(vocab.tokenize(piece.text.as_bytes(), i == 0, piece.control));
            ends.push(tokens.len());
        }
        let mut checkpoints: Vec<usize> = if self.needs_checkpoints {
            [rendered.system_end, rendered.query_end]
                .into_iter()
                .flatten()
                .filter_map(|n| ends.get(n).copied())
                .collect()
        } else {
            Vec::new()
        };
        checkpoints.dedup();
        let system_end = rendered.system_end.and_then(|n| ends.get(n).copied());
        Prompt {
            tokens,
            checkpoints,
            system_end,
        }
    }

    /// Builds and tokenizes the prompt. If it would not leave room for an answer, whole
    /// exchanges are dropped from the start — never the system prompt and never the
    /// last question; if even that does not fit, the message is too long.
    fn prompt_tokens(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDef],
    ) -> Result<Prompt, EngineError> {
        let n_ctx = self.config.n_ctx as usize;
        let room_for_answer = self.config.max_new_tokens.min(self.config.n_ctx / 2) as usize;
        let (system, rest) = match messages.split_first() {
            Some((first, rest)) if first.role == ChatRole::System => (Some(first), rest),
            _ => (None, messages),
        };
        let roles: Vec<ChatRole> = rest.iter().map(|m| m.role).collect();
        let mut longest = 0;
        for start in window_starts(&roles) {
            let window: Vec<&ChatMessage> = system
                .into_iter()
                .chain(rest.get(start..).unwrap_or_default())
                .collect();
            let prompt = self.tokenize(&prompt::render(
                self.format,
                &window,
                tools,
                self.config.thinking,
            ));
            if prompt.tokens.len() + room_for_answer <= n_ctx {
                return Ok(prompt);
            }
            longest = longest.max(prompt.tokens.len());
        }
        Err(EngineError::ContextFull {
            prompt_tokens: longest,
            n_ctx: self.config.n_ctx,
        })
    }
}

/// Where the prompt window may start among the messages after the system prompt, longest
/// window first: the whole conversation (minus leading tool results, which would have
/// lost their call), then from each later question. Never past the last question.
fn window_starts(roles: &[ChatRole]) -> Vec<usize> {
    let first = roles
        .iter()
        .position(|r| *r != ChatRole::Tool)
        .unwrap_or(roles.len());
    let mut starts = vec![first];
    starts.extend(
        roles
            .iter()
            .enumerate()
            .skip(first + 1)
            .filter(|(_, r)| **r == ChatRole::User)
            .map(|(i, _)| i),
    );
    starts
}

fn pos(i: usize) -> Result<i32, EngineError> {
    i32::try_from(i).map_err(|_| EngineError::Decode("position out of range".to_string()))
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ChatRole::{Assistant as A, Tool as T, User as U};

    #[test]
    fn windows_drop_whole_exchanges_but_never_the_last_question() {
        assert_eq!(window_starts(&[U, A, T, A, U, A, U]), [0, 4, 6]);
        // Leading tool results lost their call; the long tool result never becomes
        // the start of a window without its question.
        assert_eq!(window_starts(&[T, T, A, U]), [2, 3]);
        assert_eq!(window_starts(&[U, A, T]), [0]);
        assert_eq!(window_starts(&[]), [0]);
    }
}
