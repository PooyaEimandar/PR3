//! The memory panel (docs Sec. 4.4): what PR3 remembers, for the user to review. The
//! model can only propose memories; confirming, correcting, adding and forgetting are
//! the user's, and happen here, never through the model.

use std::sync::{MutexGuard, PoisonError};

use pr3_core::Timestamp;
use pr3_memory::{Memory, MemoryId, MemoryStore, NewMemory, Provenance, Source, Status};
use pr3_tool::SharedMemory;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::host::now;

/// The memory store, shared with the host that recalls from it.
pub struct MemoryHandle(pub SharedMemory);

/// How many memories the panel lists.
const LIMIT: usize = 1000;
/// The longest memory the user can write: a sentence or two.
const MAX_CHARS: usize = 500;

/// A memory as the panel shows it.
#[derive(Debug, Serialize)]
pub struct MemoryView {
    id: i64,
    text: String,
    /// `proposed` (by the model, not yet reviewed) or `confirmed`.
    status: &'static str,
    /// Whose information it is: `user`, or the person it is about.
    about: String,
    /// When it was recorded, in milliseconds since the epoch.
    created_at: u64,
    /// The user's words it was proposed from, if it came from a conversation.
    source: Option<String>,
    /// Whether it replaces an earlier version the user corrected.
    corrects: bool,
}

impl From<Memory> for MemoryView {
    fn from(m: Memory) -> Self {
        Self {
            id: m.id.0,
            text: m.text,
            status: match m.status {
                Status::Proposed => "proposed",
                Status::Confirmed => "confirmed",
            },
            about: m.provenance.principal,
            created_at: m.provenance.created_at.0,
            source: match m.provenance.source {
                Source::Conversation { excerpt, .. } if !excerpt.is_empty() => Some(excerpt),
                _ => None,
            },
            corrects: m.supersedes.is_some(),
        }
    }
}

fn lock(memory: &SharedMemory) -> MutexGuard<'_, dyn MemoryStore + Send + 'static> {
    memory.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the user typed, as one line, or why it cannot be stored.
fn cleaned(text: &str) -> Result<String, String> {
    let text: String = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    if text.is_empty() {
        return Err("write something to remember".to_string());
    }
    if text.chars().count() > MAX_CHARS {
        return Err(format!("keep it under {MAX_CHARS} characters"));
    }
    Ok(text)
}

/// Live memories, newest first.
pub fn list(memory: &SharedMemory, now: Timestamp) -> Result<Vec<MemoryView>, String> {
    let memories = lock(memory).recent(LIMIT, now).map_err(|e| e.to_string())?;
    Ok(memories.into_iter().map(MemoryView::from).collect())
}

/// Something the user wants remembered, confirmed from the start.
pub fn add(memory: &SharedMemory, text: &str, now: Timestamp) -> Result<i64, String> {
    let text = cleaned(text)?;
    let new = NewMemory {
        text,
        status: Status::Confirmed,
        provenance: Provenance::from_user(now),
    };
    Ok(lock(memory).remember(new).map_err(|e| e.to_string())?.0)
}

pub fn confirm(memory: &SharedMemory, id: i64, now: Timestamp) -> Result<(), String> {
    if lock(memory)
        .confirm(MemoryId(id), now)
        .map_err(|e| e.to_string())?
    {
        Ok(())
    } else {
        Err("that memory has changed; reopen Memory".to_string())
    }
}

/// The user's new wording: it replaces the memory, which stops being recalled. The same
/// wording only confirms it.
pub fn correct(memory: &SharedMemory, id: i64, text: &str, now: Timestamp) -> Result<i64, String> {
    let text = cleaned(text)?;
    let mut store = lock(memory);
    let current = store
        .get(MemoryId(id))
        .map_err(|e| e.to_string())?
        .ok_or("that memory has been forgotten")?;
    if current.text == text {
        drop(store);
        confirm(memory, id, now)?;
        return Ok(id);
    }
    Ok(store
        .correct(MemoryId(id), &text, now)
        .map_err(|e| e.to_string())?
        .0)
}

/// Erases the memory, its earlier versions and the words it came from.
pub fn forget(memory: &SharedMemory, id: i64, now: Timestamp) -> Result<(), String> {
    lock(memory)
        .forget(MemoryId(id), now)
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn store(app: &AppHandle) -> Result<SharedMemory, String> {
    app.try_state::<MemoryHandle>()
        .map(|handle| handle.0.clone())
        .ok_or_else(|| "memory is unavailable on this device".to_string())
}

#[tauri::command]
pub async fn list_memories(app: AppHandle) -> Result<Vec<MemoryView>, String> {
    list(&store(&app)?, now())
}

#[tauri::command]
pub async fn add_memory(app: AppHandle, text: String) -> Result<i64, String> {
    add(&store(&app)?, &text, now())
}

#[tauri::command]
pub async fn confirm_memory(app: AppHandle, id: i64) -> Result<(), String> {
    confirm(&store(&app)?, id, now())
}

#[tauri::command]
pub async fn correct_memory(app: AppHandle, id: i64, text: String) -> Result<i64, String> {
    correct(&store(&app)?, id, &text, now())
}

#[tauri::command]
pub async fn forget_memory(app: AppHandle, id: i64) -> Result<(), String> {
    forget(&store(&app)?, id, now())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use pr3_core::SessionId;
    use pr3_memory_sqlite::SqliteMemory;

    use super::*;

    const NOW: Timestamp = Timestamp(1_000);

    fn memory() -> SharedMemory {
        Arc::new(Mutex::new(SqliteMemory::open_in_memory().unwrap()))
    }

    fn propose(memory: &SharedMemory, text: &str, said: &str) -> i64 {
        let new = NewMemory {
            text: text.to_string(),
            status: Status::Proposed,
            provenance: Provenance::proposed(SessionId(1), said.to_string(), NOW),
        };
        lock(memory).remember(new).unwrap().0
    }

    #[test]
    fn a_proposal_is_listed_with_the_words_it_came_from_and_can_be_confirmed() {
        let m = memory();
        let id = propose(
            &m,
            "The user's favourite tea is jasmine.",
            "I love jasmine tea",
        );
        let listed = list(&m, NOW).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].status, "proposed");
        assert_eq!(listed[0].source.as_deref(), Some("I love jasmine tea"));
        assert_eq!(listed[0].about, "user");
        confirm(&m, id, NOW).unwrap();
        assert_eq!(list(&m, NOW).unwrap()[0].status, "confirmed");
    }

    #[test]
    fn a_correction_replaces_the_memory_and_the_same_wording_confirms_it() {
        let m = memory();
        let id = propose(&m, "The user's name is Ava.", "call me Ada");
        assert_eq!(
            correct(&m, id, "  The user's name is Ava.  ", NOW).unwrap(),
            id
        );
        assert_eq!(list(&m, NOW).unwrap()[0].status, "confirmed");
        let newer = correct(&m, id, "The user's name is\nAda.", NOW).unwrap();
        let listed = list(&m, NOW).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            (listed[0].id, listed[0].text.as_str()),
            (newer, "The user's name is Ada.")
        );
        assert!(listed[0].corrects);
        assert!(
            correct(&m, id, "Something else.", NOW).is_err(),
            "only the newest version"
        );
    }

    #[test]
    fn added_memories_are_confirmed_and_forgetting_removes_them() {
        let m = memory();
        let id = add(&m, "I'm allergic to peanuts.", NOW).unwrap();
        let listed = list(&m, NOW).unwrap();
        assert_eq!(
            (listed[0].status, listed[0].source.as_deref()),
            ("confirmed", None)
        );
        forget(&m, id, NOW).unwrap();
        assert!(list(&m, NOW).unwrap().is_empty());
        assert!(confirm(&m, id, NOW).is_err());
    }

    #[test]
    fn empty_or_overlong_text_is_refused() {
        let m = memory();
        assert!(add(&m, " \n\t ", NOW).is_err());
        assert!(add(&m, &"a".repeat(MAX_CHARS + 1), NOW).is_err());
        assert!(add(&m, &"a".repeat(MAX_CHARS), NOW).is_ok());
    }
}
