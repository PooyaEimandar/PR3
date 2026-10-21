//! Stops an agent that repeats itself. Ported from OpenJarvis's `LoopGuard`
//! (rust/crates/openjarvis-agents/src/loop_guard.rs): identical calls, A-B-A-B
//! ping-pong between two tools, and an overall call budget. `no_std`, so calls are
//! keyed by name and canonical arguments instead of a SHA-256 digest.

use alloc::collections::{BTreeSet, VecDeque};
use alloc::format;
use alloc::string::String;

use crate::types::ToolCall;

#[derive(Debug)]
pub(crate) struct LoopGuard {
    seen: BTreeSet<String>,
    recent: VecDeque<String>,
    calls: usize,
    max_ping_pong: usize,
    budget: usize,
}

impl LoopGuard {
    pub(crate) fn new(max_ping_pong: usize, budget: usize) -> Self {
        Self {
            seen: BTreeSet::new(),
            recent: VecDeque::new(),
            calls: 0,
            max_ping_pong: max_ping_pong.max(2),
            budget,
        }
    }

    /// Records a call; returns why the turn must stop, if it must.
    pub(crate) fn check(&mut self, call: &ToolCall) -> Option<String> {
        let key = format!("{}|{}", call.name, call.arguments);
        if !self.seen.insert(key) {
            return Some(format!(
                "loop detected: identical call to '{}' with the same arguments",
                call.name
            ));
        }

        self.recent.push_back(call.name.clone());
        while self.recent.len() > self.max_ping_pong * 2 {
            self.recent.pop_front();
        }
        let n = self.recent.len();
        if n >= 4 {
            let at = |i: usize| self.recent.get(n - i);
            if at(1) == at(3) && at(2) == at(4) && at(1) != at(2) {
                return Some(format!(
                    "loop detected: ping-pong between '{}' and '{}'",
                    at(1).map_or("", String::as_str),
                    at(2).map_or("", String::as_str)
                ));
            }
        }

        self.calls += 1;
        if self.calls > self.budget {
            return Some(format!(
                "tool budget exceeded: {} calls (budget {})",
                self.calls, self.budget
            ));
        }
        None
    }
}

impl Default for LoopGuard {
    fn default() -> Self {
        Self::new(4, 16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    fn call(name: &str, arguments: &str) -> ToolCall {
        ToolCall {
            id: "c".to_string(),
            name: name.to_string(),
            arguments: arguments.to_string(),
        }
    }

    #[test]
    fn identical_call_is_caught() {
        let mut g = LoopGuard::default();
        assert!(g.check(&call("calc", r#"{"e":"2+2"}"#)).is_none());
        assert!(g.check(&call("calc", r#"{"e":"2+2"}"#)).is_some());
    }

    #[test]
    fn different_calls_pass() {
        let mut g = LoopGuard::default();
        assert!(g.check(&call("calc", r#"{"e":"2+2"}"#)).is_none());
        assert!(g.check(&call("calc", r#"{"e":"3+3"}"#)).is_none());
    }

    #[test]
    fn ping_pong_is_caught() {
        let mut g = LoopGuard::default();
        assert!(g.check(&call("a", "1")).is_none());
        assert!(g.check(&call("b", "1")).is_none());
        assert!(g.check(&call("a", "2")).is_none());
        let stop = g.check(&call("b", "2")).unwrap_or_default();
        assert!(stop.contains("ping-pong"), "{stop}");
    }

    #[test]
    fn budget_is_enforced() {
        let mut g = LoopGuard::new(4, 2);
        assert!(g.check(&call("a", "1")).is_none());
        assert!(g.check(&call("a", "2")).is_none());
        assert!(g.check(&call("a", "3")).is_some());
    }
}
