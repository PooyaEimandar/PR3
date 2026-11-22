//! A session's history: a ring of message slots. Dropping old exchanges moves nothing,
//! and a new message is written over a dropped one in place, keeping its buffers.

use alloc::string::String;
use alloc::vec::Vec;

use crate::error::CoreError;
use crate::types::{Message, Role, Timestamp, ToolCall};

/// Objects a step let go of, kept for the next ones, so a steady-state turn allocates
/// nothing: a recycled `String` keeps its capacity. Each list is bounded; past its bound
/// an object is dropped.
#[derive(Debug, Default)]
pub(crate) struct Spare {
    calls: Vec<ToolCall>,
    strings: Vec<String>,
}

const SPARE_CALLS: usize = 64;
const SPARE_STRINGS: usize = 64;

impl Spare {
    pub(crate) fn call(&mut self) -> ToolCall {
        let mut call = self.calls.pop().unwrap_or_default();
        call.id.clear();
        call.name.clear();
        call.arguments.clear();
        call
    }

    fn put_call(&mut self, call: ToolCall) {
        if self.calls.len() < SPARE_CALLS {
            self.calls.push(call);
        }
    }

    pub(crate) fn string(&mut self) -> String {
        let mut text = self.strings.pop().unwrap_or_default();
        text.clear();
        text
    }

    fn put_string(&mut self, text: String) {
        if self.strings.len() < SPARE_STRINGS {
            self.strings.push(text);
        }
    }
}

/// The messages are `slots[head..]` then, wrapping, `slots[..]`, `len` of them; the other
/// slots hold dropped messages, which the next messages are written over.
#[derive(Debug, Default, Eq)]
pub(crate) struct History {
    slots: Vec<Message>,
    head: usize,
    len: usize,
    /// The sequence number of the first message; a message keeps its number while it
    /// stays.
    front: u64,
}

/// Equal when the same messages carry the same numbers, whatever the dropped slots hold.
impl PartialEq for History {
    fn eq(&self, other: &Self) -> bool {
        self.front == other.front && self.len == other.len && self.iter().eq(other.iter())
    }
}

impl History {
    /// The slot of the message `i` places after the first, or of the next message for
    /// `i == len`.
    fn slot(&self, i: usize) -> usize {
        let at = self.head + i;
        if at >= self.slots.len() {
            at - self.slots.len()
        } else {
            at
        }
    }

    pub(crate) fn get(&self, i: usize) -> Option<&Message> {
        if i < self.len {
            self.slots.get(self.slot(i))
        } else {
            None
        }
    }

    /// The message numbered `seq`.
    pub(crate) fn message(&self, seq: u64) -> Option<&Message> {
        self.get(usize::try_from(seq.checked_sub(self.front)?).ok()?)
    }

    /// Call `index` of the message numbered `seq`.
    pub(crate) fn call(&self, seq: u64, index: usize) -> Option<&ToolCall> {
        self.message(seq)?.tool_calls.get(index)
    }

    /// The number the message [`Self::next`] gave will take when added, and the calls
    /// written in it so far.
    pub(crate) fn pending(&self) -> (u64, &[ToolCall]) {
        let calls = if self.len < self.slots.len() {
            self.slots.get(self.slot(self.len))
        } else {
            None
        };
        let calls = calls.map_or(&[][..], |m| m.tool_calls.as_slice());
        (self.front + self.len as u64, calls)
    }

    /// The messages, oldest first, as the two runs of the ring.
    pub(crate) fn as_slices(&self) -> (&[Message], &[Message]) {
        let end = self.head + self.len;
        if end <= self.slots.len() {
            (self.slots.get(self.head..end).unwrap_or(&[]), &[])
        } else {
            (
                self.slots.get(self.head..).unwrap_or(&[]),
                self.slots.get(..end - self.slots.len()).unwrap_or(&[]),
            )
        }
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &Message> {
        let (first, second) = self.as_slices();
        first.iter().chain(second)
    }

    /// The slot the next message goes in, emptied and set to `role` at `at`. It joins
    /// the history on [`Self::commit`]; until then it is still a dropped slot, so a
    /// message given up on is simply never committed.
    pub(crate) fn next(
        &mut self,
        role: Role,
        at: Timestamp,
        spare: &mut Spare,
    ) -> Result<&mut Message, CoreError> {
        if self.len == self.slots.len() {
            // Fallible allocation at an untrusted boundary (docs Sec. 4.2).
            self.slots
                .try_reserve(1)
                .map_err(|_| CoreError::OutOfMemory)?;
            let fresh = Message::new(role, String::new(), at);
            if self.head == 0 {
                self.slots.push(fresh);
            } else {
                // Full and wrapped: the new slot goes where the next message belongs,
                // between the newest message and the oldest.
                self.slots.insert(self.head, fresh);
                self.head += 1;
            }
        }
        let at_slot = self.slot(self.len);
        let message = self.slots.get_mut(at_slot).ok_or(CoreError::OutOfMemory)?;
        message.role = role;
        message.at = at;
        message.content.clear();
        while let Some(call) = message.tool_calls.pop() {
            spare.put_call(call);
        }
        if let Some(id) = message.tool_call_id.take() {
            spare.put_string(id);
        }
        Ok(message)
    }

    /// Adds the message [`Self::next`] gave, first dropping whole exchanges — a question
    /// and everything up to the next — from the start while the history holds `max` or
    /// more. The exchange in progress is never split, so a tool result never loses its
    /// call and the current question is never dropped; the limits on rounds and calls
    /// bound its length. Returns the message's sequence number.
    pub(crate) fn commit(&mut self, max: usize) -> u64 {
        while self.len >= max {
            let next_question = self
                .iter()
                .skip(1)
                .position(|m| m.role == Role::User)
                .map(|i| i + 1);
            let Some(n) = next_question else { break };
            self.head = self.slot(n);
            self.len -= n;
            self.front += n as u64;
        }
        let seq = self.front + self.len as u64;
        self.len += 1;
        seq
    }

    /// Adds a message the host gave, as [`Self::commit`] does.
    pub(crate) fn push(
        &mut self,
        message: Message,
        max: usize,
        spare: &mut Spare,
    ) -> Result<u64, CoreError> {
        let slot = self.next(message.role, message.at, spare)?;
        *slot = message;
        Ok(self.commit(max))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use alloc::vec;

    fn texts(h: &History) -> Vec<&str> {
        h.iter().map(|m| m.content.as_str()).collect()
    }

    fn add(h: &mut History, spare: &mut Spare, role: Role, text: &str, max: usize) -> u64 {
        let m = h.next(role, Timestamp(0), spare).unwrap();
        m.content.push_str(text);
        h.commit(max)
    }

    /// Whole exchanges drop from the front, slots are written over in place, and the
    /// two runs always read as the history in order.
    #[test]
    fn exchanges_drop_whole_and_slots_are_reused() {
        let (mut h, mut spare) = (History::default(), Spare::default());
        let mut seq = 0;
        for turn in 0..20 {
            let q = alloc::format!("q{turn}");
            assert_eq!(add(&mut h, &mut spare, Role::User, &q, 5), seq);
            add(&mut h, &mut spare, Role::Tool, "t", 5);
            add(&mut h, &mut spare, Role::Assistant, "a", 5);
            seq += 3;
            let all = texts(&h);
            assert_eq!(all.last(), Some(&"a"));
            assert!(all.len() <= 6, "{all:?}");
            assert_eq!(h.message(seq - 3).unwrap().content, q);
            let (first, second) = h.as_slices();
            assert_eq!(first.len() + second.len(), h.len);
        }
        assert!(h.slots.len() <= 6, "{} slots", h.slots.len());
    }

    /// A message that is never committed leaves the history as it was.
    #[test]
    fn an_uncommitted_slot_is_not_history() {
        let (mut h, mut spare) = (History::default(), Spare::default());
        add(&mut h, &mut spare, Role::User, "q", 8);
        let m = h.next(Role::Assistant, Timestamp(1), &mut spare).unwrap();
        m.content.push_str("given up");
        assert_eq!(texts(&h), vec!["q"]);
        add(&mut h, &mut spare, Role::Assistant, "a", 8);
        assert_eq!(texts(&h), vec!["q", "a"]);
    }

    /// The ring grows while wrapped without losing order: one long exchange past the
    /// limit, after the front has moved.
    #[test]
    fn a_wrapped_ring_grows_in_order() {
        let (mut h, mut spare) = (History::default(), Spare::default());
        for text in ["q1", "a1", "q2", "a2", "q3"] {
            let role = if text.starts_with('q') {
                Role::User
            } else {
                Role::Assistant
            };
            add(&mut h, &mut spare, role, text, 4);
        }
        assert!(h.head > 0);
        let long: Vec<String> = (0..6).map(|i| alloc::format!("t{i}")).collect();
        for text in &long {
            add(&mut h, &mut spare, Role::Tool, text, 4);
        }
        let mut want = vec!["q3"];
        want.extend(long.iter().map(String::as_str));
        assert_eq!(texts(&h), want);
        assert_eq!(h.message(h.front).unwrap().content, "q3");
    }
}
