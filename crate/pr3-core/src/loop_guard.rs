//! Stops an agent that repeats itself. Ported from OpenJarvis's `LoopGuard`
//! (rust/crates/openjarvis-agents/src/loop_guard.rs): identical calls and an overall
//! call budget. `no_std`, so calls are compared by name and canonical arguments, where
//! the history keeps them, instead of by a SHA-256 digest.
//!
//! OpenJarvis's A-B-A-B ping-pong check is left out. Compared by tool name alone it
//! stops legitimate turns such as search, open, search, open; compared with arguments it
//! can never fire, because the repeated call is already caught as identical.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::types::ToolCall;

/// Where a call is: call `.1` of the message numbered `.0`.
pub(crate) type Place = (u64, usize);

/// The calls seen this turn, by where the history keeps them, so none is copied; a turn
/// holds at most `Config::max_tool_calls_per_turn` of them, so a scan is cheap.
#[derive(Debug, Default)]
pub(crate) struct LoopGuard {
    seen: Vec<Place>,
    calls: usize,
    budget: usize,
}

impl LoopGuard {
    #[cfg(test)]
    pub(crate) fn new(budget: usize) -> Self {
        let mut guard = Self::default();
        guard.reset(budget);
        guard
    }

    /// Starts a new turn, keeping the buffer.
    pub(crate) fn reset(&mut self, budget: usize) {
        self.seen.clear();
        self.calls = 0;
        self.budget = budget;
    }

    /// Records the call at `at`; `find` finds any of the turn's calls by its place.
    /// Returns why the turn must stop, if it must.
    pub(crate) fn check<'a>(
        &mut self,
        at: Place,
        find: impl Fn(Place) -> Option<&'a ToolCall>,
    ) -> Option<String> {
        if let Some(call) = find(at) {
            let same = |seen: &ToolCall| seen.arguments == call.arguments && seen.name == call.name;
            if self.seen.iter().filter_map(|&place| find(place)).any(same) {
                return Some(format!(
                    "loop detected: identical call to '{}' with the same arguments",
                    call.name
                ));
            }
        }
        self.seen.push(at);
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
#[allow(clippy::indexing_slicing)]
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

    /// Checks one message's calls in order and gives each verdict: `true` to stop.
    fn check_all(g: &mut LoopGuard, calls: &[ToolCall]) -> Vec<bool> {
        let find = |(_, index): Place| calls.get(index);
        (0..calls.len())
            .map(|index| g.check((0, index), find).is_some())
            .collect()
    }

    #[test]
    fn identical_call_is_caught() {
        let mut g = LoopGuard::new(16);
        let calls = [
            call("calc", r#"{"e":"2+2"}"#),
            call("calc", r#"{"e":"2+2"}"#),
        ];
        assert_eq!(check_all(&mut g, &calls), [false, true]);
    }

    #[test]
    fn alternating_tools_with_new_arguments_pass() {
        let mut g = LoopGuard::new(16);
        let calls = [
            call("search", "a"),
            call("open", "x"),
            call("search", "b"),
            call("open", "y"),
        ];
        assert_eq!(check_all(&mut g, &calls), [false; 4]);
    }

    #[test]
    fn budget_is_enforced() {
        let mut g = LoopGuard::new(2);
        let calls = [call("a", "1"), call("a", "2"), call("a", "3")];
        assert_eq!(check_all(&mut g, &calls), [false, false, true]);
    }

    /// A call is compared with those of earlier messages too, each found at its own
    /// place; the same name with other arguments, or other name with the same, is new.
    #[test]
    fn calls_of_earlier_messages_are_found_by_place() {
        let mut g = LoopGuard::new(16);
        let messages = [
            [call("calc", "{}"), call("open", "x")],
            [call("calc", "{\"e\":1}"), call("search", "x")],
            [call("open", "y"), call("open", "x")],
        ];
        let find = |(seq, index): Place| messages.get(seq as usize)?.get(index);
        let mut verdicts = Vec::new();
        for seq in 0..3 {
            for index in 0..2 {
                verdicts.push(g.check((seq, index), find).is_some());
            }
        }
        assert_eq!(verdicts, [false, false, false, false, false, true]);
    }

    #[test]
    fn a_reset_forgets_the_last_turn() {
        let mut g = LoopGuard::new(16);
        let calls = [call("calc", "{}")];
        assert_eq!(check_all(&mut g, &calls), [false]);
        g.reset(16);
        assert_eq!(check_all(&mut g, &calls), [false]);
    }
}
