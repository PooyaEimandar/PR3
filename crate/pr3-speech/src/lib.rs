//! PR3's speech (docs Sec. 4.3, "Speech, voice first"): push-to-talk speech-to-text for
//! voice notes, and spoken replies. Each half sits behind a trait, [`Recognizer`] and
//! [`Voice`], so engines swap the way models do.
//!
//! Platform engines come first, and only on-device ones: on Apple devices the Speech
//! framework with on-device recognition required and the system's voices
//! ([`open_platform`]); on Android the app reaches `SpeechRecognizer` and
//! `TextToSpeech` through its own plugin, because they need the app's activity, and
//! implements these traits too. Where no on-device recognizer exists, none is offered:
//! audio never leaves the device, and none is kept.

#![deny(unsafe_code)]

#[cfg(target_vendor = "apple")]
mod apple;

use std::fmt;

/// What a listening session reports while it runs.
#[derive(Debug, Clone, Copy)]
pub enum Heard<'a> {
    /// The transcript so far; each replaces the last.
    Partial(&'a str),
    /// The session ended on its own, before `stop`: the recognizer failed or the
    /// microphone was lost. `stop` reports the same error.
    Failed(&'a SpeechError),
}

/// Receives what a listening session hears.
pub type OnHeard = Box<dyn Fn(Heard<'_>) + Send + Sync>;

pub trait Recognizer: Send + Sync {
    /// Starts listening on the microphone. The first time, the system asks for the
    /// microphone and speech recognition; this blocks until the user answers, so it is
    /// never called on the main thread. Fails with [`SpeechError::Busy`] while another
    /// session runs.
    fn start(&self, on_heard: OnHeard) -> Result<(), SpeechError>;

    /// Stops listening and returns the final transcript, empty if nothing was heard.
    fn stop(&self) -> Result<String, SpeechError>;

    /// Stops listening and discards what was heard.
    fn cancel(&self);

    /// The language it recognizes, as a BCP 47 tag such as `en-US`.
    fn locale(&self) -> &str;
}

pub trait Voice: Send + Sync {
    /// Speaks `text` with an on-device voice, interrupting anything still being
    /// spoken. Returns once speaking has started.
    fn speak(&self, text: &str) -> Result<(), SpeechError>;

    /// Stops speaking at once.
    fn stop(&self);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeechError {
    /// The user said no to what is named: the microphone or speech recognition.
    Denied(String),
    /// No on-device engine for this language or platform, or not right now.
    Unavailable(String),
    /// A listening session is already running.
    Busy,
    /// No listening session is running.
    NotListening,
    /// The platform's engine failed.
    Platform(String),
}

impl fmt::Display for SpeechError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Denied(what) => write!(f, "PR3 may not use {what}; allow it in Settings"),
            Self::Unavailable(why) => f.write_str(why),
            Self::Busy => f.write_str("already listening"),
            Self::NotListening => f.write_str("not listening"),
            Self::Platform(why) => write!(f, "speech: {why}"),
        }
    }
}

impl std::error::Error for SpeechError {}

/// This device's speech engines. Each may be missing on its own: a voice may exist
/// where recognition does not.
pub struct Engines {
    pub recognizer: Result<Box<dyn Recognizer>, SpeechError>,
    pub voice: Result<Box<dyn Voice>, SpeechError>,
}

/// This device's on-device recognizer and voice, for `locale` (a BCP 47 tag; `None`
/// for the system's language). The voice speaks the recognizer's language. Android's
/// engines live in the app (see the crate documentation).
pub fn open_platform(locale: Option<&str>) -> Engines {
    #[cfg(target_vendor = "apple")]
    {
        let recognizer = apple::AppleRecognizer::open(locale);
        let spoken = match &recognizer {
            Ok(r) => Some(r.locale().to_string()),
            Err(_) => locale.map(str::to_string),
        };
        Engines {
            recognizer: recognizer.map(|r| Box::new(r) as Box<dyn Recognizer>),
            voice: Ok(Box::new(apple::AppleVoice::new(spoken.as_deref()))),
        }
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        let _ = locale;
        let none = || SpeechError::Unavailable("no on-device speech engine here yet".to_string());
        Engines {
            recognizer: Err(none()),
            voice: Err(none()),
        }
    }
}

/// The languages this device recognizes on the device, as BCP 47 tags such as `en-US`:
/// the ones [`open_platform`] can open. Empty where none can be listed here; Android's
/// list comes from the app's plugin.
pub fn platform_languages() -> Vec<String> {
    #[cfg(target_vendor = "apple")]
    {
        apple::languages()
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        Vec::new()
    }
}

/// A reply as it should sound: Markdown's markers dropped (emphasis, headings, bullets,
/// quotes, code fences and backticks), links read as their text, and each line its own
/// sentence.
pub fn speakable(text: &str) -> String {
    let mut lines = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("```") {
            continue;
        }
        let line = line.trim_start_matches(['#', '>']).trim_start();
        let line = ["- ", "* ", "+ "]
            .iter()
            .find_map(|bullet| line.strip_prefix(bullet))
            .unwrap_or(line);
        let line = emphasis_removed(&links_read(line));
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if !line.is_empty() {
            lines.push(line);
        }
    }
    let mut spoken = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            if !spoken.ends_with(['.', '!', '?', ':', ';', ',']) {
                spoken.push('.');
            }
            spoken.push(' ');
        }
        spoken.push_str(line);
    }
    spoken
}

/// `[text](url)` → `text`.
fn links_read(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let link = after.find("](").and_then(|close| {
            let url = &after[close + 2..];
            url.find(')').map(|end| (&after[..close], &url[end + 1..]))
        });
        match link {
            Some((text, tail)) if !text.contains('[') => {
                out.push_str(&rest[..open]);
                out.push_str(text);
                rest = tail;
            }
            _ => {
                out.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Drops backticks, `**` and `__`, and a `*` that opens or closes emphasis; keeps
/// `2 * 3` and `snake_case`.
fn emphasis_removed(line: &str) -> String {
    let line = line.replace("**", "").replace("__", "").replace('`', "");
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    for (i, &c) in chars.iter().enumerate() {
        if c == '*' {
            let before = i.checked_sub(1).map(|j| chars[j]);
            let after = chars.get(i + 1).copied();
            let opens =
                before.is_none_or(char::is_whitespace) && after.is_some_and(|a| !a.is_whitespace());
            let closes = before.is_some_and(|b| !b.is_whitespace())
                && after.is_none_or(|a| a.is_whitespace() || a.is_ascii_punctuation());
            if opens || closes {
                continue;
            }
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_is_read_as_plain_sentences() {
        assert_eq!(
            speakable("**Hello** there, *friend*."),
            "Hello there, friend."
        );
        assert_eq!(
            speakable("# Plan\n\n- buy milk\n- call Alex\n> later"),
            "Plan. buy milk. call Alex. later"
        );
        assert_eq!(
            speakable("See [the docs](https://example.com/a_b) or [x]."),
            "See the docs or [x]."
        );
        assert_eq!(
            speakable("Run `cargo test`:\n```sh\ncargo test\n```"),
            "Run cargo test: cargo test"
        );
    }

    #[test]
    fn arithmetic_and_identifiers_survive() {
        assert_eq!(speakable("2 * 3 = 6"), "2 * 3 = 6");
        assert_eq!(speakable("set max_mb to 64"), "set max_mb to 64");
        assert_eq!(speakable("a*b"), "a*b");
    }

    #[test]
    fn nothing_to_say_is_empty() {
        assert_eq!(speakable(""), "");
        assert_eq!(speakable("```\n```\n\n  "), "");
    }
}
