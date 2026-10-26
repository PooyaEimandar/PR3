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

    /// Ends the stream: a character cut off at the end becomes U+FFFD instead of
    /// disappearing.
    pub(crate) fn finish(&mut self) -> String {
        if self.pending.is_empty() {
            String::new()
        } else {
            self.pending.clear();
            char::REPLACEMENT_CHARACTER.to_string()
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
    /// For output that starts inside a think block the prompt already opened.
    pub(crate) fn inside_think() -> Self {
        Self {
            state: ThinkState::InsideThink,
            ..Self::default()
        }
    }

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

/// Removes `open … close` blocks from streamed text wherever they appear, keeping their
/// contents. Used for `<tool_call>` blocks: the user sees the answer, the caller gets the
/// calls. Partial tags are held back until it is clear whether they open a block.
#[derive(Debug)]
pub(crate) struct BlockExtractor {
    open: &'static str,
    close: &'static str,
    buffer: String,
    inside: bool,
    blocks: Vec<String>,
}

impl BlockExtractor {
    pub(crate) fn new(open: &'static str, close: &'static str) -> Self {
        Self {
            open,
            close,
            buffer: String::new(),
            inside: false,
            blocks: Vec::new(),
        }
    }

    /// Adds streamed text and returns the part that is visible now.
    pub(crate) fn push(&mut self, text: &str) -> String {
        self.buffer.push_str(text);
        let mut visible = String::new();
        loop {
            if self.inside {
                let Some(end) = self.buffer.find(self.close) else {
                    return visible;
                };
                self.blocks.push(self.buffer[..end].to_string());
                self.buffer.drain(..end + self.close.len());
                self.inside = false;
            } else if let Some(start) = self.buffer.find(self.open) {
                visible.push_str(&self.buffer[..start]);
                self.buffer.drain(..start + self.open.len());
                self.inside = true;
            } else {
                // Hold back a suffix that could still grow into the opening tag. The tag
                // is ASCII, so the cut is always on a character boundary.
                let keep = (1..self.open.len())
                    .rev()
                    .find(|&n| self.buffer.ends_with(&self.open[..n]))
                    .unwrap_or(0);
                let emit = self.buffer.len() - keep;
                visible.push_str(&self.buffer[..emit]);
                self.buffer.drain(..emit);
                return visible;
            }
        }
    }

    /// Ends the stream: returns the remaining visible text. An unterminated block is
    /// still kept as a block — a model that forgets the closing tag still meant to call.
    pub(crate) fn finish(&mut self) -> String {
        let rest = std::mem::take(&mut self.buffer);
        if self.inside {
            self.blocks.push(rest);
            self.inside = false;
            String::new()
        } else {
            rest
        }
    }

    pub(crate) fn blocks(&self) -> &[String] {
        &self.blocks
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

    #[test]
    fn a_character_cut_off_at_the_end_is_not_lost() {
        let mut stream = Utf8Stream::default();
        let bytes = "ok👋".as_bytes();
        assert_eq!(stream.push(&bytes[..bytes.len() - 1]), "ok");
        assert_eq!(stream.finish(), "\u{FFFD}");
        assert_eq!(stream.finish(), "");
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
    fn output_that_starts_inside_think_hides_the_reasoning() {
        let mut f = ThinkFilter::inside_think();
        let shown = f.push("weighing it up\n</think>\n\nYes.") + &f.finish();
        assert_eq!(shown, "Yes.");
    }

    #[test]
    fn unterminated_think_is_dropped() {
        assert_eq!(run(&["<think>never closed"]), "");
    }

    fn extract(pieces: &[&str]) -> (String, Vec<String>) {
        let mut x = BlockExtractor::new("<tool_call>", "</tool_call>");
        let mut out: String = pieces.iter().map(|p| x.push(p)).collect();
        out.push_str(&x.finish());
        (out, x.blocks().to_vec())
    }

    #[test]
    fn tool_call_blocks_are_removed_and_kept() {
        let (text, blocks) = extract(&[
            "Sure.\n<tool_",
            "call>\n{\"name\":\"a\"}\n</tool_call>",
            " done",
        ]);
        assert_eq!(text, "Sure.\n done");
        assert_eq!(blocks, vec!["\n{\"name\":\"a\"}\n".to_string()]);
    }

    #[test]
    fn text_resembling_a_tag_is_released() {
        let (text, blocks) = extract(&["a <tool", "s are fine"]);
        assert_eq!(text, "a <tools are fine");
        assert!(blocks.is_empty());
    }

    #[test]
    fn unterminated_block_still_counts() {
        let (text, blocks) = extract(&["<tool_call>{\"name\":\"a\"}"]);
        assert_eq!(text, "");
        assert_eq!(blocks, vec!["{\"name\":\"a\"}".to_string()]);
    }
}
