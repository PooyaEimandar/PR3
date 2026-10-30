//! A task as two files (docs Sec. 4.8): its chat as Markdown, to read, and its record,
//! chat and lifecycle events as JSON, to check. The events are the ones memory.sqlite
//! chains by hash; each comes with the hash before it, so it can be recomputed without
//! the database, and `chain_intact` says whether the whole chain checked on this device.

use std::fs;
use std::path::{Path, PathBuf};

use pr3_core::Timestamp;
use pr3_memory_sqlite::{TaskEvent, TaskState};
use pr3_tool::clock;
use serde::Serialize;
use serde_json::json;
use tauri::Manager;

use super::{Entry, Page, Task, TaskCard, state};

/// What the JSON file is; a change that breaks its readers changes this.
const FORMAT: &str = "pr3.task/1";
/// How each event's hash is made (pr3-memory-sqlite's `task_events`).
const EVENT_HASH: &str = "SHA-256, in hex, of five parts, each prefixed with its length as \
    an 8-byte big-endian integer: `previous` as hex text; the task id and `at` as 8-byte \
    big-endian integers; `event` as UTF-8; `parent` as an 8-byte big-endian integer, or \
    empty when null.";
/// Longest file name, in characters, before its extension.
const STEM_CHARS: usize = 48;

pub struct Export {
    /// The name both files share, without its extension.
    pub stem: String,
    pub markdown: String,
    pub json: String,
}

/// One step of the chat, with when it was said.
#[derive(Serialize)]
struct Step<'a> {
    at: u64,
    #[serde(flatten)]
    entry: &'a Entry,
}

/// `now` is when, and in which UTC offset, the user exports it.
pub(super) fn build(
    task: &Task,
    card: &TaskCard,
    parent: Option<&Task>,
    events: &[TaskEvent],
    intact: Option<bool>,
    (now, offset): (Timestamp, i32),
) -> Export {
    Export {
        stem: stem(task),
        markdown: markdown(task, card, parent, (now, offset)),
        json: json(task, events, intact, (now, offset)),
    }
}

/// Where exports wait for the share sheet, in the app's cache folder.
pub fn dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    Ok(cache.join("export"))
}

/// Writes both files into `dir`, emptied first so an earlier export does not linger.
pub fn write(dir: &Path, export: &Export) -> Result<Vec<PathBuf>, String> {
    clear(dir);
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    for (extension, text) in [("md", &export.markdown), ("json", &export.json)] {
        let file = dir.join(format!("{}.{extension}", export.stem));
        fs::write(&file, text).map_err(|e| e.to_string())?;
        files.push(file);
    }
    Ok(files)
}

/// Removes what earlier exports left: they hold the chat in plain text.
pub fn clear(dir: &Path) {
    if let Err(e) = fs::remove_dir_all(dir)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("PR3: an earlier export is still in {}: {e}", dir.display());
    }
}

fn markdown(task: &Task, card: &TaskCard, parent: Option<&Task>, at: (Timestamp, i32)) -> String {
    let (now, offset) = at;
    let page = match card.page {
        Page::Todo => "To Do",
        Page::Doing => "Doing",
        Page::Done => "Done",
    };
    let changed = clock::minute(task.updated_at, offset);
    let mut md = format!("# {}\n\n{page} · {} · {changed}\n", task.title, card.line);
    if let Some(parent) = parent {
        md += &format!("\nFollows up: {}\n", parent.title);
    }
    md += &format!("\nExported from PR3 on {}.\n", clock::describe(now, offset));
    for (at, entry) in &task.chat {
        let when = clock::minute(*at, offset);
        md += &match entry {
            Entry::User { text } => format!("\n**You** · {when}\n\n{text}\n"),
            Entry::Reply { text } => format!("\n**PR3** · {when}\n\n{text}\n"),
            Entry::Tool {
                name,
                arguments,
                success,
                output,
            } => {
                let how = if *success { "done" } else { "failed" };
                format!(
                    "\n**PR3 ran {name}** ({how}) · {when}\n\n{}\n\n{}\n",
                    fence(arguments),
                    fence(output)
                )
            }
            Entry::Failed { reason } => format!("\n**Didn't finish** · {when}\n\n{reason}\n"),
        };
    }
    md
}

fn json(task: &Task, events: &[TaskEvent], intact: Option<bool>, at: (Timestamp, i32)) -> String {
    let (now, offset) = at;
    let (state, reason, waiting) = match state(task) {
        TaskState::Working => ("working", None, None),
        TaskState::Waiting(call) => (
            "waiting",
            None,
            Some(json!({
                "tool": call.tool,
                "arguments": call.arguments,
                "until": call.until.0,
            })),
        ),
        TaskState::Done => ("done", None, None),
        TaskState::Failed(reason) => ("failed", Some(reason), None),
        TaskState::Expired => ("expired", None, None),
    };
    let chat: Vec<Step<'_>> = task
        .chat
        .iter()
        .map(|(at, entry)| Step { at: at.0, entry })
        .collect();
    let events: Vec<_> = events
        .iter()
        .map(|e| {
            json!({
                "at": e.at.0,
                "event": e.event,
                "parent": e.parent,
                "previous": e.previous,
                "hash": e.hash,
            })
        })
        .collect();
    let file = json!({
        "format": FORMAT,
        "exported_at": now.0,
        "utc_offset_minutes": offset,
        "task": {
            "id": task.id.0,
            "title": task.title,
            "parent": task.parent.map(|p| p.0),
            "state": state,
            "reason": reason,
            "waiting": waiting,
            "updated_at": task.updated_at.0,
            // The model's summary, and how many steps of the chat it covers.
            "summary": task.summary.as_ref().map(|s| json!({"text": s.text, "steps": s.steps})),
        },
        "chat": chat,
        "events": events,
        "event_hash": EVENT_HASH,
        "chain_intact": intact,
    });
    serde_json::to_string_pretty(&file).unwrap_or_default()
}

/// `pr3-` and the title's words, joined by dashes; the id when it has none.
fn stem(task: &Task) -> String {
    let words: Vec<String> = task
        .title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        return format!("pr3-task-{}", task.id.0);
    }
    let mut stem = "pr3".to_string();
    for word in words {
        if stem.chars().count() + 1 + word.chars().count() > STEM_CHARS {
            break;
        }
        stem.push('-');
        stem.push_str(&word);
    }
    stem
}

/// `text` as a code block, fenced by more backticks than it holds in a row.
fn fence(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        run = if c == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let ticks = "`".repeat(longest.max(2) + 1);
    format!("{ticks}\n{text}\n{ticks}")
}

#[cfg(test)]
mod tests {
    use pr3_core::{SessionId, ToolCall};
    use pr3_memory_sqlite::SqliteMemory;
    use std::sync::{Arc, Mutex};

    use super::super::Tasks;
    use super::*;
    use crate::host::TurnEvent;

    // 2026-10-20 16:15:00 UTC.
    const T0: u64 = 1_792_512_900_000;

    fn at(minutes: u64) -> Timestamp {
        Timestamp(T0 + minutes * 60 * 1000)
    }

    #[test]
    fn a_task_exports_as_its_chat_and_its_checkable_events() {
        let store = Arc::new(Mutex::new(SqliteMemory::open_in_memory().unwrap()));
        let mut tasks = Tasks::load(store, 30, at(0)).unwrap();
        let (first, _) = tasks.begin("Plan the trip", at(0));
        tasks.end(first, Ok("Lisbon in May.".to_string()), at(0));
        tasks.follow_up(first, at(1)).unwrap();
        let (id, _) = tasks.begin("Email Alex the plan", at(2));
        let call = ToolCall {
            id: "c1".to_string(),
            name: "send_email".to_string(),
            arguments: r#"{"to":["Alex"],"body":"```code```"}"#.to_string(),
        };
        tasks.waiting(id, &call, at(4), at(2));
        tasks.record(
            id,
            &TurnEvent::Tool {
                name: "send_email".to_string(),
                arguments: call.arguments.clone(),
                success: true,
                output: "SENT".to_string(),
            },
            at(3),
        );
        tasks.end(id, Ok("Sent to Alex Rivera.".to_string()), at(3));

        let export = tasks.export(id, at(30), 210).unwrap();
        assert_eq!(export.stem, "pr3-email-alex-the-plan");
        let md = &export.markdown;
        assert!(
            md.starts_with(
                "# Email Alex the plan\n\nDone · Sent to Alex Rivera. · 2026-10-20 19:48\n"
            ),
            "{md}"
        );
        assert!(md.contains("\nFollows up: Plan the trip\n"), "{md}");
        assert!(md.contains("Exported from PR3 on Tuesday 2026-10-20 20:15 (UTC+03:30)."));
        assert!(md.contains("**You** · 2026-10-20 19:47\n\nEmail Alex the plan\n"));
        // A fence longer than any run of backticks the text holds.
        assert!(
            md.contains("**PR3 ran send_email** (done) · 2026-10-20 19:48\n\n````\n"),
            "{md}"
        );
        assert!(md.contains("**PR3** · 2026-10-20 19:48\n\nSent to Alex Rivera.\n"));

        let file: serde_json::Value = serde_json::from_str(&export.json).unwrap();
        assert_eq!(file["format"], FORMAT);
        assert_eq!(file["task"]["state"], "done");
        assert_eq!(file["task"]["parent"], first.0);
        assert_eq!(file["chat"][0]["type"], "user");
        assert_eq!(file["chat"][1]["output"], "SENT");
        assert_eq!(file["chain_intact"], true);
        let events: Vec<&str> = file["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["event"].as_str().unwrap())
            .collect();
        assert_eq!(events, ["opened", "waiting", "working", "done"]);
        assert_eq!(file["events"][0]["parent"], first.0);
        // Each event checks against the hash before it, as `event_hash` says.
        for e in file["events"].as_array().unwrap() {
            let parent = e["parent"].as_u64().map(|p| p.to_be_bytes());
            let hash = pr3_core::trusted::Hash::of(&[
                e["previous"].as_str().unwrap().as_bytes(),
                &id.0.to_be_bytes(),
                &e["at"].as_u64().unwrap().to_be_bytes(),
                e["event"].as_str().unwrap().as_bytes(),
                parent.as_ref().map_or(&[][..], |p| &p[..]),
            ]);
            assert_eq!(e["hash"], hash.to_hex());
        }
    }

    #[test]
    fn a_task_kept_nowhere_still_exports_without_events() {
        let mut tasks = Tasks::default();
        let (id, _) = tasks.begin("¿Qué tal?", at(0));
        tasks.end(id, Err("no model".to_string()), at(0));
        let export = tasks.export(id, at(1), 0).unwrap();
        assert_eq!(export.stem, "pr3-qué-tal");
        let file: serde_json::Value = serde_json::from_str(&export.json).unwrap();
        assert_eq!(file["task"]["state"], "failed");
        assert_eq!(file["task"]["reason"], "no model");
        assert_eq!(file["events"], serde_json::json!([]));
        assert_eq!(file["chain_intact"], serde_json::Value::Null);
        assert!(tasks.export(SessionId(id.0 + 1), at(1), 0).is_err());
        let titled = Task {
            title: "!!!".to_string(),
            ..tasks.tasks.remove(0)
        };
        assert_eq!(stem(&titled), format!("pr3-task-{}", id.0));
    }
}
