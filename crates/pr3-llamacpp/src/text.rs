//! Model-output handling that needs no model: UTF-8 reassembly and `<think>` filtering.

/// Reassembles UTF-8 from token pieces. A multi-byte character can be split across
/// tokens; bytes are held back until they form complete characters.
#[derive(Debug, Default)]
pub(crate) struct Utf8Stream {
    pending: Vec<u8>,
}

impl Utf8Stream {
    /// Adds bytes and returns whatever complete text is now available. Invalid bytes
    /// become U+FFFD rather than an error: model output is untrusted, not fatal.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    out.push_str(text);
                    self.pending.clear();
                    return out;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    out.push_str(&String::from_utf8_lossy(&self.pending[..valid]));
                    match e.error_len() {
                        // Incomplete character at the end: wait for the next piece.
                        None => {
                            self.pending.drain(..valid);
                            return out;
                        }
                        // Genuinely invalid bytes: replace them and keep going.
                        Some(bad) => {
                            out.push(char::REPLACEMENT_CHARACTER);
                            self.pending.drain(..valid + bad);
                        }
                    }
                }
            }
        }
    }
}

const THINK_OPEN: &str = "<think>";
const THINK_CLOSE: &str = "</think>";

/// Hides a leading `<think>…</think>` block from streamed output. Qwen3 may emit one even
/// in non-thinking mode (often empty); the user sees only the answer that follows.
#[derive(Debug, Default)]
pub(crate) struct ThinkFilter {
    buffer: String,
    state: ThinkState,
    /// Visible text has started; until then leading whitespace is dropped, so the
    /// answer never starts with the blank lines that follow a think block.
    started: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum ThinkState {
    /// Not yet known whether the output opens with a think block.
    #[default]
    Undecided,
    InsideThink,
    /// Past any think block; everything is visible.
    Visible,
}

impl ThinkFilter {
    /// Adds streamed text and returns the part that is safe to show now.
    pub(crate) fn push(&mut self, text: &str) -> String {
        self.buffer.push_str(text);
        loop {
            match self.state {
                ThinkState::Undecided => {
                    let trimmed = self.buffer.trim_start();
                    if trimmed.starts_with(THINK_OPEN) {
                        self.state = ThinkState::InsideThink;
                    } else if THINK_OPEN.starts_with(trimmed) {
                        // Could still become "<think>": wait for more.
                        return String::new();
                    } else {
                        self.state = ThinkState::Visible;
                    }
                }
                ThinkState::InsideThink => {
                    let Some(end) = self.buffer.find(THINK_CLOSE) else {
                        return String::new();
                    };
                    let rest = self.buffer[end + THINK_CLOSE.len()..]
                        .trim_start()
                        .to_string();
                    self.buffer = rest;
                    self.state = ThinkState::Visible;
                }
                ThinkState::Visible => {
                    let text = std::mem::take(&mut self.buffer);
                    return self.visible(text);
                }
            }
        }
    }

    fn visible(&mut self, text: String) -> String {
        if self.started {
            return text;
        }
        let trimmed = text.trim_start();
        if trimmed.is_empty() {
            return String::new();
        }
        self.started = true;
        trimmed.to_string()
    }

    /// Flushes held-back text at the end of generation. An unterminated think block is
    /// dropped; text that only looked like the start of one is returned.
    pub(crate) fn finish(&mut self) -> String {
        match self.state {
            ThinkState::InsideThink => String::new(),
            _ => {
                let text = std::mem::take(&mut self.buffer);
                self.visible(text)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_split_across_pieces_is_reassembled() {
        let bytes = "سلام 👋".as_bytes();
        let mut stream = Utf8Stream::default();
        let mut out = String::new();
        for b in bytes {
            out.push_str(&stream.push(&[*b]));
        }
        assert_eq!(out, "سلام 👋");
    }

    #[test]
    fn invalid_utf8_is_replaced_not_fatal() {
        let mut stream = Utf8Stream::default();
        assert_eq!(stream.push(&[b'a', 0xFF, b'b']), "a\u{FFFD}b");
    }

    fn run(pieces: &[&str]) -> String {
        let mut filter = ThinkFilter::default();
        let mut out: String = pieces.iter().map(|p| filter.push(p)).collect();
        out.push_str(&filter.finish());
        out
    }

    #[test]
    fn empty_think_block_is_hidden() {
        assert_eq!(
            run(&["<think>\n\n</think>\n\n", "Hello", " there"]),
            "Hello there"
        );
    }

    #[test]
    fn think_block_split_across_pieces_is_hidden() {
        assert_eq!(
            run(&["<th", "ink>plan", " steps</th", "ink>", " Answer"]),
            "Answer"
        );
    }

    #[test]
    fn text_without_think_passes_through() {
        assert_eq!(run(&["Hi", ", ", "you"]), "Hi, you");
    }

    #[test]
    fn short_text_that_could_start_a_tag_is_flushed() {
        assert_eq!(run(&["<th"]), "<th");
    }

    #[test]
    fn unterminated_think_is_dropped() {
        assert_eq!(run(&["<think>never closed"]), "");
    }
}
