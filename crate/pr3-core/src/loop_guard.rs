//! Stops an agent that repeats itself. Ported from OpenJarvis's `LoopGuard`
//! (rust/crates/openjarvis-agents/src/loop_guard.rs): identical calls and an overall
//! call budget. `no_std`, so calls are keyed by name and canonical arguments instead of
//! a SHA-256 digest.
//!
//! OpenJarvis's A-B-A-B ping-pong check is left out. Compared by tool name alone it
//! stops legitimate turns such as search, open, search, open; compared with arguments it
//! can never fire, because the repeated call is already caught as identical.

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;

use crate::types::ToolCall;

#[derive(Debug)]
pub(crate) struct LoopGuard {
    seen: BTreeSet<String>,
    calls: usize,
    budget: usize,
}

impl LoopGuard {
    pub(crate) fn new(budget: usize) -> Self {
        Self {
            seen: BTreeSet::new(),
            calls: 0,
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
        let mut g = LoopGuard::new(16);
        assert!(g.check(&call("calc", r#"{"e":"2+2"}"#)).is_none());
        assert!(g.check(&call("calc", r#"{"e":"2+2"}"#)).is_some());
    }

    #[test]
    fn alternating_tools_with_new_arguments_pass() {
        let mut g = LoopGuard::new(16);
        for (name, args) in [
            ("search", "a"),
            ("open", "x"),
            ("search", "b"),
            ("open", "y"),
        ] {
            assert!(g.check(&call(name, args)).is_none(), "{name} {args}");
        }
    }

    #[test]
    fn budget_is_enforced() {
        let mut g = LoopGuard::new(2);
        assert!(g.check(&call("a", "1")).is_none());
        assert!(g.check(&call("a", "2")).is_none());
        assert!(g.check(&call("a", "3")).is_some());
    }
}
