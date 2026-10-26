//! What the engine's llama.cpp context already holds, and how much of a new prompt can
//! reuse it. A tool round resends the whole conversation, and every call starts with the
//! same system prompt and tool list, so most of a prompt has usually been processed
//! before. Pure bookkeeping, tested without a model.
//!
//! Attention-only models (Qwen3) can drop cached tokens from any position. Hybrid models
//! (Qwen3.5 and later) also carry a recurrent state that cannot be rewound, so the
//! engine saves that state where later prompts are expected to differ — checkpoints —
//! and rewinds to the latest one that still matches: after the system turn, which every
//! call shares, and, for templates that rewrite earlier answers, after each question.

/// The recurrent state after the context's first `pos` tokens.
pub(crate) struct Checkpoint<S> {
    pub pos: usize,
    pub state: S,
}

/// The first checkpoint (the system prompt and tools, shared by every call) plus this
/// many recent ones. Each holds a full recurrent state: about 52 MB for Qwen3.5-4B and
/// 156 MB for Qwen3.8-27B.
const RECENT_CHECKPOINTS: usize = 1;

pub(crate) struct Cache<T, S> {
    /// Tokens whose effect is in the context, at positions `0..len`.
    pub tokens: Vec<T>,
    /// In ascending position order.
    pub checkpoints: Vec<Checkpoint<S>>,
}

/// How to bring the context in line with a new prompt.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Rewind {
    /// The cache is a prefix of the prompt; only the rest needs processing.
    None,
    /// Drop cached tokens from position `to` on.
    Trim { to: usize },
    /// Restore `checkpoints[index]`, then drop cached tokens after it.
    Restore { index: usize },
    /// Nothing reusable.
    Clear,
}

impl<T: PartialEq, S> Cache<T, S> {
    pub fn new() -> Self {
        Self {
            tokens: Vec::new(),
            checkpoints: Vec::new(),
        }
    }

    /// How to reuse the cache for `prompt`. The prompt's last token is always processed
    /// again, because sampling needs its logits. `can_trim` is false for models whose
    /// state cannot be cut back to an arbitrary position.
    pub fn plan(&self, prompt: &[T], can_trim: bool) -> Rewind {
        let common = common_prefix(&self.tokens, prompt).min(prompt.len().saturating_sub(1));
        if common == self.tokens.len() {
            Rewind::None
        } else if common == 0 {
            Rewind::Clear
        } else if can_trim {
            Rewind::Trim { to: common }
        } else {
            match self.checkpoints.iter().rposition(|c| c.pos <= common) {
                Some(index) => Rewind::Restore { index },
                None => Rewind::Clear,
            }
        }
    }

    /// Forgets everything from position `to` on, including later checkpoints.
    pub fn truncate(&mut self, to: usize) {
        self.tokens.truncate(to);
        self.checkpoints.retain(|c| c.pos <= to);
    }

    pub fn clear(&mut self) {
        self.tokens.clear();
        self.checkpoints.clear();
    }

    /// Records a checkpoint at the current end of the cache, evicting the oldest recent
    /// one if there are too many; the first checkpoint is always kept.
    pub fn add_checkpoint(&mut self, state: S) {
        let pos = self.tokens.len();
        if self.checkpoints.last().is_some_and(|c| c.pos >= pos) {
            return;
        }
        self.checkpoints.push(Checkpoint { pos, state });
        if self.checkpoints.len() > RECENT_CHECKPOINTS + 1 {
            self.checkpoints.remove(1);
        }
    }
}

pub(crate) fn common_prefix<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache(tokens: &[u32], checkpoints: &[usize]) -> Cache<u32, ()> {
        Cache {
            tokens: tokens.to_vec(),
            checkpoints: checkpoints
                .iter()
                .map(|&pos| Checkpoint { pos, state: () })
                .collect(),
        }
    }

    #[test]
    fn a_continued_conversation_only_appends() {
        let c = cache(&[1, 2, 3], &[]);
        assert_eq!(c.plan(&[1, 2, 3, 4, 5], true), Rewind::None);
        assert_eq!(c.plan(&[1, 2, 3, 4, 5], false), Rewind::None);
        assert_eq!(cache(&[], &[]).plan(&[1, 2], false), Rewind::None);
    }

    #[test]
    fn attention_models_trim_where_the_prompt_differs() {
        let c = cache(&[1, 2, 3, 4], &[]);
        assert_eq!(c.plan(&[1, 2, 9, 9], true), Rewind::Trim { to: 2 });
        assert_eq!(c.plan(&[7, 8], true), Rewind::Clear);
    }

    #[test]
    fn the_last_prompt_token_is_always_processed_again() {
        let c = cache(&[1, 2, 3], &[]);
        assert_eq!(c.plan(&[1, 2, 3], true), Rewind::Trim { to: 2 });
        assert_eq!(c.plan(&[1, 2], true), Rewind::Trim { to: 1 });
    }

    #[test]
    fn hybrid_models_restore_the_latest_checkpoint_that_still_matches() {
        let c = cache(&[1, 2, 3, 4, 5, 6], &[2, 4]);
        assert_eq!(
            c.plan(&[1, 2, 3, 4, 5, 9], false),
            Rewind::Restore { index: 1 }
        );
        assert_eq!(c.plan(&[1, 2, 3, 9], false), Rewind::Restore { index: 0 });
        assert_eq!(c.plan(&[1, 9], false), Rewind::Clear);
    }

    #[test]
    fn truncating_drops_later_checkpoints() {
        let mut c = cache(&[1, 2, 3, 4, 5], &[1, 3, 5]);
        c.truncate(3);
        assert_eq!(c.tokens, [1, 2, 3]);
        let kept: Vec<usize> = c.checkpoints.iter().map(|c| c.pos).collect();
        assert_eq!(kept, [1, 3]);
    }

    #[test]
    fn the_first_checkpoint_survives_eviction() {
        let mut c = cache(&[], &[]);
        for token in 0..6 {
            c.tokens.push(token);
            c.add_checkpoint(());
        }
        let kept: Vec<usize> = c.checkpoints.iter().map(|c| c.pos).collect();
        assert_eq!(kept, [1, 6]);
        c.add_checkpoint(());
        assert_eq!(c.checkpoints.len(), 2, "no duplicate at the same position");
    }
}
