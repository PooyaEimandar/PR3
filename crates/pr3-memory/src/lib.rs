//! What PR3 remembers about its user, where each memory came from, and how it may be
//! used (Roadmap §4.4). Types and the store interface only; storage adapters such as
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

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

pub use pr3_core::{SessionId, Timestamp};

/// The principal a memory is about when nobody else is named.
pub const USER: &str = "user";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MemoryId(pub i64);

impl fmt::Display for MemoryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Where a memory came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Said in a conversation. `excerpt` is what the user said in that turn, so every
    /// memory can be traced back to the user's own words.
    Conversation { session: SessionId, excerpt: String },
    /// Entered or corrected by the user directly.
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Written by the model: a proposal the user has not confirmed.
    Proposed,
    /// Entered or confirmed by the user.
    Confirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Privacy {
    Personal,
    /// Never leaves the device and is never sent to a remote model.
    Sensitive,
}

/// What the user has agreed a memory may be used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consent {
    /// Stays on this device: not synced, not sent to a node. The only scope until
    /// device sync exists (Roadmap §6.3).
    DeviceOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retention {
    Keep,
    /// No longer recalled after this time.
    Until(Timestamp),
}

/// The provenance every memory carries (Roadmap §4.4).
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Memory {
    pub id: MemoryId,
    pub text: String,
    pub status: Status,
    pub provenance: Provenance,
    /// The memory this one corrects.
    pub supersedes: Option<MemoryId>,
}

/// A memory to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMemory {
    pub text: String,
    pub status: Status,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

    /// The user's correction: stores `text` as a confirmed memory that supersedes
    /// `id`, which stops being recalled.
    fn correct(&mut self, id: MemoryId, text: &str, at: Timestamp)
    -> Result<MemoryId, MemoryError>;

    /// Erases a memory's text, and its source's words if nothing else needs them,
    /// leaving only a record that something was forgotten. Returns false if there was
    /// nothing to forget.
    fn forget(&mut self, id: MemoryId, at: Timestamp) -> Result<bool, MemoryError>;
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
