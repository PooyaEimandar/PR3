//! What PR3 remembers about its user, where each memory came from, and how it may be
//! used (docs Sec. 4.4). Types and the store interface only; storage adapters such as
//! `pr3-memory-sqlite` implement [`MemoryStore`].
//!
//! The model never decides what is true about the user. Anything it writes is a
//! [`Status::Proposed`] memory, traced to the words the user actually said; the user's
//! own entries and corrections are [`Status::Confirmed`] and supersede proposals.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
#![cfg_attr(test, allow(clippy::disallowed_macros))] // tests may assert (clippy.toml)

extern crate alloc;

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

pub use pr3_core::{SessionId, Timestamp};

/// The principal a memory is about when nobody else is named.
pub const USER: &str = "user";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct MemoryId(pub i64);

impl fmt::Display for MemoryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Where a memory came from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Source {
    /// Said in a conversation. `excerpt` is what the user said in that turn, so every
    /// memory can be traced back to the user's own words.
    Conversation { session: SessionId, excerpt: String },
    /// Entered or corrected by the user directly.
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Status {
    /// Written by the model: a proposal the user has not confirmed.
    Proposed,
    /// Entered or confirmed by the user.
    Confirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Privacy {
    Personal,
    /// Never leaves the device and is never sent to a remote model.
    Sensitive,
}

/// What the user has agreed a memory may be used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Consent {
    /// Local by default. Pairing enables continuity transport; an explicit per-item
    /// grant selects a confirmed Personal fact to share with that peer.
    /// Sensitive facts remain ineligible even with that grant (docs Sec. 6.3).
    DeviceOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Retention {
    Keep,
    /// No longer recalled after this time.
    Until(Timestamp),
}

/// The provenance every memory carries (docs Sec. 4.4).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Provenance {
    pub source: Source,
    /// Whose information this is: [`USER`], or the person it is about.
    pub principal: String,
    pub created_at: Timestamp,
    /// When the fact became true, if different from when it was recorded.
    pub valid_at: Timestamp,
    /// 0–100.
    pub confidence: u8,
    pub privacy: Privacy,
    pub consent: Consent,
    pub retention: Retention,
}

impl Provenance {
    /// A memory the model proposes from what the user said this turn.
    pub fn proposed(session: SessionId, excerpt: String, at: Timestamp) -> Self {
        Self {
            source: Source::Conversation { session, excerpt },
            principal: String::from(USER),
            created_at: at,
            valid_at: at,
            confidence: 70,
            privacy: Privacy::Personal,
            consent: Consent::DeviceOnly,
            retention: Retention::Keep,
        }
    }

    /// A memory the user entered themselves.
    pub fn from_user(at: Timestamp) -> Self {
        Self {
            source: Source::User,
            confidence: 100,
            ..Self::proposed(SessionId(0), String::new(), at)
        }
    }
}

/// A stored memory.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Memory {
    pub id: MemoryId,
    pub text: String,
    pub status: Status,
    pub provenance: Provenance,
    /// The memory this one corrects.
    pub supersedes: Option<MemoryId>,
}

/// A memory to store.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NewMemory {
    pub text: String,
    pub status: Status,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MemoryError(pub String);

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for MemoryError {}

/// Persistent memory. A memory is *live* — recalled — unless it was forgotten,
/// corrected by a newer one, or is past its retention.
pub trait MemoryStore {
    /// Stores a memory and returns its id. If a live memory with the same text exists,
    /// returns that one instead, confirming it if the new one is confirmed.
    fn remember(&mut self, memory: NewMemory) -> Result<MemoryId, MemoryError>;

    /// Live memories matching `query`, best match first. Empty if nothing matches,
    /// including when the query has no searchable words.
    fn search(&self, query: &str, limit: usize, now: Timestamp)
    -> Result<Vec<Memory>, MemoryError>;

    /// Live memories, newest first.
    fn recent(&self, limit: usize, now: Timestamp) -> Result<Vec<Memory>, MemoryError>;

    /// Any memory still holding text, live or not.
    fn get(&self, id: MemoryId) -> Result<Option<Memory>, MemoryError>;

    /// The user's confirmation of a live memory: a proposal becomes
    /// [`Status::Confirmed`] and fully trusted, keeping its source. Returns false if no
    /// live memory has that id; confirming a confirmed memory changes nothing.
    fn confirm(&mut self, id: MemoryId, now: Timestamp) -> Result<bool, MemoryError>;

    /// The user's correction: stores `text` as a confirmed memory that supersedes
    /// `id`, which stops being recalled. A memory that was already corrected is
    /// refused: correct its newest version instead.
    fn correct(&mut self, id: MemoryId, text: &str, at: Timestamp)
    -> Result<MemoryId, MemoryError>;

    /// Erases a memory's text, the earlier versions it corrected, and its source's
    /// words if no other memory still needs them, leaving only a record that something
    /// was forgotten. Forgetting an earlier version leaves its correction, which is the
    /// user's own text. Returns false if there was nothing to forget.
    fn forget(&mut self, id: MemoryId, at: Timestamp) -> Result<bool, MemoryError>;
}

/// Conservative candidates for user review, never permission to overwrite a fact.
/// Matching words suggest a shared subject; they do not establish a contradiction.
pub fn possible_updates(
    store: &dyn MemoryStore,
    text: &str,
    principal: &str,
    exclude: Option<MemoryId>,
    now: Timestamp,
) -> Result<Vec<Memory>, MemoryError> {
    let words = subject_words(text);
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let query = words.iter().cloned().collect::<Vec<_>>().join(" ");
    let normalized = comparable(text);
    Ok(store
        .search(&query, 24, now)?
        .into_iter()
        .filter(|m| {
            if Some(m.id) == exclude || m.provenance.principal != principal {
                return false;
            }
            if comparable(&m.text) == normalized {
                return false;
            }
            let other = subject_words(&m.text);
            let shared = words.intersection(&other).count();
            shared > 0 && shared * 2 >= words.len().min(other.len())
        })
        .take(5)
        .collect())
}

fn comparable(text: &str) -> String {
    text.replace(['\u{2018}', '\u{2019}'], "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', '!'])
        .to_lowercase()
}

fn subject_words(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.chars().count() >= 3 && !w.chars().all(char::is_numeric))
        .filter(|w| {
            !matches!(
                w.as_str(),
                "the"
                    | "and"
                    | "for"
                    | "that"
                    | "this"
                    | "with"
                    | "from"
                    | "user"
                    | "was"
                    | "are"
                    | "has"
                    | "have"
                    | "now"
                    | "new"
                    | "not"
                    | "instead"
                    | "actually"
                    | "please"
                    | "remember"
                    | "about"
                    | "their"
                    | "they"
                    | "will"
                    | "been"
                    | "moved"
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_entries_are_fully_trusted_proposals_are_not() {
        let at = Timestamp(5);
        let proposed = Provenance::proposed(SessionId(1), String::from("I'm Ada"), at);
        let user = Provenance::from_user(at);
        assert_eq!(proposed.principal, USER);
        assert!(proposed.confidence < user.confidence);
        assert_eq!(user.source, Source::User);
        assert_eq!((user.created_at, user.valid_at), (at, at));
        assert_eq!(alloc::format!("{}", MemoryId(3)), "#3");
    }
}
