//! The memory panel (docs Sec. 4.4): what PR3 remembers, for the user to review. The
//! model can only propose memories; confirming, correcting, adding and forgetting are
//! the user's, and happen here, never through the model.

use std::sync::{MutexGuard, PoisonError};

use pr3_core::Timestamp;
use pr3_memory::{
    Memory, MemoryId, MemoryStore, NewMemory, Provenance, Source, Status, USER, possible_updates,
};
use pr3_tool::SharedMemory;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::host::{Host, now};

/// The memory store, shared with the host that recalls from it.
pub struct MemoryHandle(pub SharedMemory);

/// How many memories the panel lists.
const LIMIT: usize = 1000;
/// The longest memory the user can write: a sentence or two.
const MAX_CHARS: usize = 500;

mod commands;
pub(crate) mod semantic;
#[cfg(test)]
use commands::capture;
pub use commands::command;

#[derive(Clone, Copy)]
pub enum Command<'a> {
    Save { text: &'a str, separate: bool },
    Correct { id: i64, text: &'a str },
    Invalid,
}

impl Command<'_> {
    pub fn tool(&self) -> &'static str {
        match self {
            Self::Save { .. } => "save_memory",
            _ => "correct_memory",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Candidate {
    pub id: i64,
    pub text: String,
}

impl From<Memory> for Candidate {
    fn from(m: Memory) -> Self {
        Self {
            id: m.id.0,
            text: m.text,
        }
    }
}

pub struct Applied {
    pub tool: &'static str,
    pub arguments: String,
    pub reply: String,
    pub corrected: bool,
}

#[cfg(test)]
pub fn apply(
    memory: &SharedMemory,
    command: Command<'_>,
    now: Timestamp,
) -> Result<Applied, String> {
    apply_reviewed(memory, command, now, None)
}

pub fn apply_reviewed(
    memory: &SharedMemory,
    command: Command<'_>,
    now: Timestamp,
    review: Option<semantic::Review>,
) -> Result<Applied, String> {
    match command {
        Command::Save { text, separate } => {
            let text = cleaned(text)?;
            let mut notice = None;
            if !separate {
                let review = match review {
                    Some(review) => review,
                    None => semantic::Review {
                        candidates: possible_updates(&*lock(memory), &text, USER, None, now)
                            .map_err(|e| e.to_string())?
                            .into_iter()
                            .map(Candidate::from)
                            .collect(),
                        ..semantic::Review::default()
                    },
                };
                notice = review.notice;
                if !review.candidates.is_empty() || review.incomplete {
                    return Ok(Applied {
                        tool: "review_memory", arguments: serde_json::json!({"fact":text,"candidates":review.candidates,"notice":notice}).to_string(),
                        reply: "This may update an existing memory. Choose which memory to replace, or save it separately. No memory was changed.".into(), corrected: false,
                    });
                }
            }
            let id = add(memory, &text, now)?;
            Ok(Applied {
                tool: "save_memory",
                arguments: serde_json::json!({"fact":text}).to_string(),
                reply: match notice {
                    Some(notice) => format!("Saved to memory as #{id}: {text}\n{notice}"),
                    None => format!("Saved to memory as #{id}: {text}"),
                },
                corrected: false,
            })
        }
        Command::Correct { id, text } => {
            let text = cleaned(text)?;
            let new = correct(memory, id, &text, now)?;
            Ok(Applied {
                tool: "correct_memory",
                arguments: serde_json::json!({"id":id,"fact":text}).to_string(),
                reply: format!("Memory #{id} updated as #{new}: {text}"),
                corrected: true,
            })
        }
        Command::Invalid => Err("Use Correct memory #3: followed by the complete new fact.".into()),
    }
}

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
    /// Possible earlier facts; the user decides whether these are an update.
    conflicts: Vec<Candidate>,
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
            conflicts: Vec::new(),
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
    let store = lock(memory);
    let memories = store.recent(LIMIT, now).map_err(|e| e.to_string())?;
    memories
        .into_iter()
        .map(|m| {
            let conflicts = if m.status == Status::Proposed {
                possible_updates(&*store, &m.text, &m.provenance.principal, Some(m.id), now)
                    .map_err(|e| e.to_string())?
            } else {
                Vec::new()
            };
            let mut view = MemoryView::from(m);
            view.conflicts = conflicts.into_iter().map(Candidate::from).collect();
            Ok(view)
        })
        .collect()
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
pub async fn confirm_memory(app: AppHandle, id: i64) -> Result<Confirmation, String> {
    app.state::<Host>().confirm_memory(id).await
}

#[derive(Debug, Serialize)]
pub struct Confirmation {
    pub id: i64,
    pub confirmed: bool,
    pub conflicts: Vec<Candidate>,
    pub notice: Option<String>,
}

pub fn review_target(memory: &SharedMemory, id: i64, at: Timestamp) -> Result<Memory, String> {
    lock(memory)
        .recent(LIMIT, at)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|m| m.id.0 == id)
        .ok_or_else(|| "that memory has changed; reopen Memory".into())
}

pub fn confirm_checked(
    memory: &SharedMemory,
    id: i64,
    separate: bool,
    now: Timestamp,
) -> Result<(), String> {
    let mut store = lock(memory);
    let current = store
        .get(MemoryId(id))
        .map_err(|e| e.to_string())?
        .ok_or("that memory has been forgotten")?;
    if !separate
        && !possible_updates(
            &*store,
            &current.text,
            &current.provenance.principal,
            Some(current.id),
            now,
        )
        .map_err(|e| e.to_string())?
        .is_empty()
    {
        return Err(
            "This may update another memory. Choose Replace or Keep both in Memory.".into(),
        );
    }
    if !store
        .confirm(MemoryId(id), now)
        .map_err(|e| e.to_string())?
    {
        return Err("that memory has changed; reopen Memory".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn keep_memory_separately(app: AppHandle, id: i64) -> Result<(), String> {
    confirm_checked(&store(&app)?, id, true, now())
}

#[tauri::command]
pub async fn correct_memory(app: AppHandle, id: i64, text: String) -> Result<i64, String> {
    app.state::<Host>().correct_memory(id, &text).await
}

/// Forgetting a memory also erases every task's summary, which may have drawn on it.
#[tauri::command]
pub async fn forget_memory(app: AppHandle, id: i64) -> Result<(), String> {
    forget(&store(&app)?, id, now())?;
    match app.try_state::<Host>() {
        Some(host) => host.forget_summaries(),
        None => Ok(()),
    }
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
    fn updates_require_a_choice_and_correction_commands_cannot_guess_a_target() {
        let m = memory();
        let old = add(&m, "The project meeting is at 14:30.", NOW).unwrap();
        assert_eq!(
            apply(
                &m,
                command("Remember that THE project meeting is at 14:30").unwrap(),
                NOW
            )
            .unwrap()
            .tool,
            "save_memory",
            "punctuation and case do not turn a duplicate into an update"
        );
        let other = add(&m, "The dentist meeting is at 09:00.", NOW).unwrap();
        let applied = apply(
            &m,
            command("Remember that the project meeting is at 15:00.").unwrap(),
            NOW,
        )
        .unwrap();
        assert_eq!(applied.tool, "review_memory");
        let review: serde_json::Value = serde_json::from_str(&applied.arguments).unwrap();
        assert!(
            review["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["id"] == old)
        );
        assert_eq!(list(&m, NOW).unwrap().len(), 2, "review writes nothing");
        let changed = apply(
            &m,
            command(&format!(
                "Correct memory #{old}: The project meeting is at 15:00."
            ))
            .unwrap(),
            NOW,
        )
        .unwrap();
        assert!(changed.corrected);
        assert_eq!(list(&m, NOW).unwrap().len(), 2);
        assert!(list(&m, NOW).unwrap().iter().any(|m| m.id == other));
        assert!(
            !list(&m, NOW)
                .unwrap()
                .iter()
                .any(|m| m.text.contains("14:30"))
        );
        assert!(
            apply(
                &m,
                command(&format!("Correct memory #{old}: stale edit")).unwrap(),
                NOW
            )
            .is_err()
        );
        for text in [
            "Correct memory #x: bad",
            "Correct memory #0: bad",
            "Correct memory #1",
            "Correct memory #1:",
        ] {
            assert!(apply(&m, command(text).unwrap(), NOW).is_err());
        }
        for text in [
            "Do not correct memory #1: tea",
            "How do I correct memory #1?",
            "Tell me a story about memory",
        ] {
            assert!(command(text).is_none());
        }
    }

    #[test]
    fn proposals_cannot_confirm_conflicting_facts_without_an_explicit_choice() {
        let m = memory();
        let old = add(&m, "The project meeting is at 14:30.", NOW).unwrap();
        let proposed = propose(
            &m,
            "The project meeting is at 15:00.",
            "The meeting moved to 15:00",
        );
        let view = list(&m, NOW).unwrap();
        assert_eq!(
            view.iter().find(|m| m.id == proposed).unwrap().conflicts[0].id,
            old
        );
        assert!(confirm_checked(&m, proposed, false, NOW).is_err());
        assert!(
            pr3_tool::notes_for(&m, "project meeting", SessionId(1), NOW)
                .iter()
                .all(|s| !s.contains("15:00"))
        );
        correct(&m, old, "The project meeting is at 15:00.", NOW).unwrap();
        let view = list(&m, NOW).unwrap();
        assert_eq!(
            view.len(),
            1,
            "the duplicate proposal retires with the old fact"
        );
        assert_eq!(view[0].status, "confirmed");
        assert!(view[0].corrects);
        let next = propose(
            &m,
            "The project meeting is at 16:00.",
            "A different meeting",
        );
        confirm_checked(&m, next, true, NOW).unwrap();
        assert_eq!(
            list(&m, NOW).unwrap().len(),
            2,
            "Keep both is the user's explicit choice"
        );
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

    #[test]
    fn only_explicit_capture_commands_store_the_users_exact_fact() {
        assert_eq!(
            capture("Please REMEMBER that The meeting is at 14:30."),
            Some("The meeting is at 14:30.")
        );
        assert_eq!(capture("remember: Café at 3"), Some("Café at 3"));
        assert_eq!(capture("remember:secret"), Some("secret"));
        assert_eq!(capture("save to memory: hello"), Some("hello"));
        assert_eq!(capture("remember that"), Some(""));
        for text in [
            "remember what I said",
            "remember to email Alex",
            "do you remember that meeting?",
            "remember thatcher",
        ] {
            assert_eq!(capture(text), None, "{text}");
        }
    }
}
