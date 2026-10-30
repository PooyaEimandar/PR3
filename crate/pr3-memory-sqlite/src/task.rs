//! Tasks on SQLite (docs Sec. 4.8): each task's record, its chat as the conversation
//! showed it, and the events of its lifecycle. The chat is the user's record: deleting
//! the task erases it, and `secure_delete` overwrites the pages it was on. The events
//! hold states and references, never text, so they outlive the task; each carries the
//! SHA-256 of the one before it, so an edited, removed or reordered event shows.

use pr3_core::trusted::Hash;
use pr3_memory::{MemoryError, Timestamp};
use rusqlite::{OptionalExtension, Row, Transaction, params};

use crate::{SqliteMemory, db_time, error, unsigned};

/// Where a stored task stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskState {
    /// A turn was running for it.
    Working,
    /// A confirmation card was waiting for the user's OK.
    Waiting(WaitingCall),
    Done,
    /// Its last turn did not finish, for this reason.
    Failed(String),
    /// Its confirmation card ran out before the user answered; nothing was done.
    Expired,
}

/// A consequential call waiting for the user's OK, as the runtime validated it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingCall {
    pub tool: String,
    pub arguments: String,
    /// When its confirmation card runs out.
    pub until: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRecord {
    /// When it opened, in milliseconds; also its session in the runtime.
    pub id: u64,
    pub title: String,
    /// The task it follows up.
    pub parent: Option<u64>,
    pub state: TaskState,
    pub updated_at: Timestamp,
}

/// One step of a task's chat, as the conversation showed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    User(String),
    Reply(String),
    Tool {
        name: String,
        arguments: String,
        success: bool,
        output: String,
    },
    /// Why a turn failed.
    Failed(String),
}

/// One event of a task's lifecycle, with the hash it chains from, so it can be checked
/// on its own: `hash` is the SHA-256 of `previous`, the task, `at`, `event` and `parent`
/// (see `task_events`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskEvent {
    pub at: Timestamp,
    /// 'opened', 'working', 'waiting', 'done', 'failed', 'expired', 'deleted' or 'erased'.
    pub event: String,
    /// For 'opened': the task it follows up.
    pub parent: Option<u64>,
    /// The hash of the event logged before it, of whichever task; empty for the first.
    pub previous: String,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTask {
    pub record: TaskRecord,
    /// Oldest first.
    pub chat: Vec<(Timestamp, Said)>,
}

impl SqliteMemory {
    /// Stores a task's record, adding it if it is new. A new task, and each change of
    /// state, is logged as an event.
    pub fn save_task(&mut self, task: &TaskRecord) -> Result<(), MemoryError> {
        let id = db_id(task.id)?;
        let parent = task.parent.map(db_id).transpose()?;
        let updated_at = db_time(task.updated_at)?;
        let state = state_name(&task.state);
        let (reason, waiting) = match &task.state {
            TaskState::Failed(reason) => (Some(reason.as_str()), None),
            TaskState::Waiting(call) => (None, Some(call)),
            _ => (None, None),
        };
        let until = waiting.map(|call| db_time(call.until)).transpose()?;
        let tx = self.conn.transaction().map_err(error)?;
        let before: Option<String> = tx
            .query_row("SELECT state FROM tasks WHERE id = ?1", params![id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(error)?;
        tx.execute(
            "INSERT INTO tasks(id, title, parent, state, reason, waiting_tool,
                 waiting_arguments, waiting_until, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET title = excluded.title,
                 parent = excluded.parent, state = excluded.state,
                 reason = excluded.reason, waiting_tool = excluded.waiting_tool,
                 waiting_arguments = excluded.waiting_arguments,
                 waiting_until = excluded.waiting_until, updated_at = excluded.updated_at",
            params![
                id,
                task.title,
                parent,
                state,
                reason,
                waiting.map(|call| call.tool.as_str()),
                waiting.map(|call| call.arguments.as_str()),
                until,
                updated_at,
            ],
        )
        .map_err(error)?;
        match before {
            None => log_event(&tx, id, updated_at, "opened", parent)?,
            Some(before) if before != state => log_event(&tx, id, updated_at, state, None)?,
            Some(_) => {}
        }
        tx.commit().map_err(error)
    }

    /// Adds a step to a task's chat.
    pub fn add_said(&mut self, task: u64, at: Timestamp, said: &Said) -> Result<(), MemoryError> {
        let (kind, text, tool, arguments, success) = match said {
            Said::User(text) => ("user", text, None, None, None),
            Said::Reply(text) => ("reply", text, None, None, None),
            Said::Tool {
                name,
                arguments,
                success,
                output,
            } => ("tool", output, Some(name), Some(arguments), Some(*success)),
            Said::Failed(reason) => ("failed", reason, None, None, None),
        };
        self.conn
            .execute(
                "INSERT INTO messages(task_id, at, kind, text, tool, arguments, success)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    db_id(task)?,
                    db_time(at)?,
                    kind,
                    text,
                    tool,
                    arguments,
                    success
                ],
            )
            .map_err(error)?;
        Ok(())
    }

    /// Every stored task with its chat, oldest first.
    pub fn tasks(&self) -> Result<Vec<StoredTask>, MemoryError> {
        let mut records = self
            .conn
            .prepare(
                "SELECT id, title, parent, state, reason, waiting_tool, waiting_arguments,
                     waiting_until, updated_at
                 FROM tasks ORDER BY id",
            )
            .map_err(error)?;
        let records = records
            .query_map([], record)
            .map_err(error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(error)?;
        let mut chat = self
            .conn
            .prepare(
                "SELECT at, kind, text, tool, arguments, success FROM messages
                 WHERE task_id = ?1 ORDER BY id",
            )
            .map_err(error)?;
        records
            .into_iter()
            .map(|record| {
                let chat = chat
                    .query_map(params![db_id(record.id)?], said)
                    .map_err(error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(error)?;
                Ok(StoredTask { record, chat })
            })
            .collect()
    }

    /// Deletes a task and its chat. Its events stay, with one more saying so.
    pub fn delete_task(&mut self, id: u64, at: Timestamp) -> Result<bool, MemoryError> {
        let tx = self.conn.transaction().map_err(error)?;
        let deleted = remove(&tx, db_id(id)?, db_time(at)?, "deleted")?;
        tx.commit().map_err(error)?;
        Ok(deleted)
    }

    /// Erases the finished tasks - done, or expired - last changed before `before`,
    /// with their chats: how long finished tasks are kept. Unfinished ones stay until
    /// they finish. Returns the ids erased.
    pub fn erase_tasks_finished_before(
        &mut self,
        before: Timestamp,
        at: Timestamp,
    ) -> Result<Vec<u64>, MemoryError> {
        let at = db_time(at)?;
        let tx = self.conn.transaction().map_err(error)?;
        let ids = {
            let mut statement = tx
                .prepare(
                    "SELECT id FROM tasks
                     WHERE state IN ('done', 'expired') AND updated_at < ?1 ORDER BY id",
                )
                .map_err(error)?;
            statement
                .query_map(params![db_time(before)?], |r| r.get::<_, i64>(0))
                .map_err(error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(error)?
        };
        for &id in &ids {
            remove(&tx, id, at, "erased")?;
        }
        tx.commit().map_err(error)?;
        Ok(ids.into_iter().map(unsigned).collect())
    }

    /// A task's events, oldest first. Each hash is [`Hash::of`] these parts, in order: the
    /// previous hash as hex text, the task id and `at` as 8-byte big-endian integers, the
    /// event name, and the parent as an 8-byte big-endian integer (empty when there is
    /// none); each part is prefixed with its length as an 8-byte big-endian integer.
    pub fn task_events(&self, task: u64) -> Result<Vec<TaskEvent>, MemoryError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT at, event, parent, hash, COALESCE((SELECT p.hash FROM task_events p
                     WHERE p.id < e.id ORDER BY p.id DESC LIMIT 1), '')
                 FROM task_events e WHERE task_id = ?1 ORDER BY id",
            )
            .map_err(error)?;
        let rows = statement
            .query_map(params![db_id(task)?], |r| {
                Ok(TaskEvent {
                    at: Timestamp(unsigned(r.get(0)?)),
                    event: r.get(1)?,
                    parent: r.get::<_, Option<i64>>(2)?.map(unsigned),
                    hash: r.get(3)?,
                    previous: r.get(4)?,
                })
            })
            .map_err(error)?;
        rows.collect::<Result<_, _>>().map_err(error)
    }

    /// Whether every task event still matches its hash: false once one is edited,
    /// removed from the middle or moved.
    pub fn task_events_intact(&self) -> Result<bool, MemoryError> {
        let mut statement = self
            .conn
            .prepare("SELECT task_id, at, event, parent, hash FROM task_events ORDER BY id")
            .map_err(error)?;
        let rows = statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<i64>>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .map_err(error)?;
        let mut previous = String::new();
        for row in rows {
            let (task, at, event, parent, hash) = row.map_err(error)?;
            if event_hash(&previous, task, at, &event, parent) != hash {
                return Ok(false);
            }
            previous = hash;
        }
        Ok(true)
    }
}

/// Deletes a task; its chat goes with it (`ON DELETE CASCADE`).
fn remove(tx: &Transaction<'_>, id: i64, at: i64, event: &str) -> Result<bool, MemoryError> {
    let removed = tx
        .execute("DELETE FROM tasks WHERE id = ?1", params![id])
        .map_err(error)?
        == 1;
    if removed {
        log_event(tx, id, at, event, None)?;
    }
    Ok(removed)
}

fn log_event(
    tx: &Transaction<'_>,
    task: i64,
    at: i64,
    event: &str,
    parent: Option<i64>,
) -> Result<(), MemoryError> {
    let previous: String = tx
        .query_row(
            "SELECT hash FROM task_events ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(error)?
        .unwrap_or_default();
    tx.execute(
        "INSERT INTO task_events(task_id, at, event, parent, hash) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            task,
            at,
            event,
            parent,
            event_hash(&previous, task, at, event, parent)
        ],
    )
    .map_err(error)?;
    Ok(())
}

fn event_hash(previous: &str, task: i64, at: i64, event: &str, parent: Option<i64>) -> String {
    let parent = parent.map(i64::to_be_bytes);
    Hash::of(&[
        previous.as_bytes(),
        &task.to_be_bytes(),
        &at.to_be_bytes(),
        event.as_bytes(),
        parent.as_ref().map_or(&[][..], |p| &p[..]),
    ])
    .to_hex()
}

fn state_name(state: &TaskState) -> &'static str {
    match state {
        TaskState::Working => "working",
        TaskState::Waiting(_) => "waiting",
        TaskState::Done => "done",
        TaskState::Failed(_) => "failed",
        TaskState::Expired => "expired",
    }
}

/// Reads a row of `tasks`, in the column order `tasks()` selects.
fn record(r: &Row<'_>) -> rusqlite::Result<TaskRecord> {
    let state = match r.get::<_, String>(3)?.as_str() {
        "working" => TaskState::Working,
        "waiting" => TaskState::Waiting(WaitingCall {
            tool: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
            arguments: r.get::<_, Option<String>>(6)?.unwrap_or_default(),
            until: Timestamp(r.get::<_, Option<i64>>(7)?.map_or(0, unsigned)),
        }),
        "done" => TaskState::Done,
        "expired" => TaskState::Expired,
        _ => TaskState::Failed(r.get::<_, Option<String>>(4)?.unwrap_or_default()),
    };
    Ok(TaskRecord {
        id: unsigned(r.get(0)?),
        title: r.get(1)?,
        parent: r.get::<_, Option<i64>>(2)?.map(unsigned),
        state,
        updated_at: Timestamp(unsigned(r.get(8)?)),
    })
}

/// Reads a row of `messages`, in the column order `tasks()` selects.
fn said(r: &Row<'_>) -> rusqlite::Result<(Timestamp, Said)> {
    let text: String = r.get(2)?;
    let said = match r.get::<_, String>(1)?.as_str() {
        "user" => Said::User(text),
        "reply" => Said::Reply(text),
        "tool" => Said::Tool {
            name: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
            arguments: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
            success: r.get::<_, Option<bool>>(5)?.unwrap_or(false),
            output: text,
        },
        _ => Said::Failed(text),
    };
    Ok((Timestamp(unsigned(r.get(0)?)), said))
}

fn db_id(id: u64) -> Result<i64, MemoryError> {
    i64::try_from(id).map_err(|_| MemoryError(format!("task {id} out of range")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: u64, state: TaskState, at: u64) -> TaskRecord {
        TaskRecord {
            id,
            title: format!("task {id}"),
            parent: None,
            state,
            updated_at: Timestamp(at),
        }
    }

    fn events(m: &SqliteMemory) -> Vec<(i64, String)> {
        let mut statement = m
            .conn
            .prepare("SELECT task_id, event FROM task_events ORDER BY id")
            .unwrap();
        statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn a_task_and_its_chat_round_trip_and_each_change_of_state_is_an_event() {
        let mut m = SqliteMemory::open_in_memory().unwrap();
        let waiting = WaitingCall {
            tool: "send_email".to_string(),
            arguments: r#"{"to":"Alex"}"#.to_string(),
            until: Timestamp(220),
        };
        m.save_task(&task(1, TaskState::Working, 100)).unwrap();
        m.add_said(1, Timestamp(100), &Said::User("Email Alex".to_string()))
            .unwrap();
        m.save_task(&task(1, TaskState::Waiting(waiting.clone()), 110))
            .unwrap();
        let stored = m.tasks().unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].record.state, TaskState::Waiting(waiting));

        let tool = Said::Tool {
            name: "send_email".to_string(),
            arguments: r#"{"to":"Alex"}"#.to_string(),
            success: true,
            output: "SENT".to_string(),
        };
        m.add_said(1, Timestamp(120), &tool).unwrap();
        m.add_said(1, Timestamp(121), &Said::Reply("Sent.".to_string()))
            .unwrap();
        m.save_task(&TaskRecord {
            parent: Some(7),
            ..task(1, TaskState::Done, 121)
        })
        .unwrap();
        // The same state again changes the record, not the events.
        m.save_task(&TaskRecord {
            parent: Some(7),
            ..task(1, TaskState::Done, 130)
        })
        .unwrap();
        m.save_task(&task(2, TaskState::Failed("no model".to_string()), 140))
            .unwrap();

        let stored = m.tasks().unwrap();
        assert_eq!(stored.len(), 2);
        assert_eq!(
            stored[0].record,
            TaskRecord {
                parent: Some(7),
                ..task(1, TaskState::Done, 130)
            }
        );
        assert_eq!(
            stored[0].chat,
            [
                (Timestamp(100), Said::User("Email Alex".to_string())),
                (Timestamp(120), tool),
                (Timestamp(121), Said::Reply("Sent.".to_string())),
            ]
        );
        assert_eq!(
            stored[1].record.state,
            TaskState::Failed("no model".to_string())
        );
        let events = events(&m);
        let names: Vec<(i64, &str)> = events
            .iter()
            .map(|(task, event)| (*task, event.as_str()))
            .collect();
        assert_eq!(
            names,
            [(1, "opened"), (1, "waiting"), (1, "done"), (2, "opened")]
        );
        assert!(m.task_events_intact().unwrap());
    }

    #[test]
    fn deleting_a_task_erases_its_chat_from_the_file_but_keeps_its_events() {
        let path =
            std::env::temp_dir().join(format!("pr3-tasks-delete-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let mut m = SqliteMemory::open(&path).unwrap();
            m.save_task(&TaskRecord {
                title: "Quokkabridge receipts".to_string(),
                ..task(1, TaskState::Working, 100)
            })
            .unwrap();
            m.add_said(
                1,
                Timestamp(100),
                &Said::User("Find my Quokkabridge receipts".to_string()),
            )
            .unwrap();
            m.add_said(
                1,
                Timestamp(101),
                &Said::Reply("Two from Zanzibarnia.".to_string()),
            )
            .unwrap();
            m.save_task(&task(2, TaskState::Done, 102)).unwrap();
            assert!(m.delete_task(1, Timestamp(200)).unwrap());
            assert!(!m.delete_task(1, Timestamp(201)).unwrap());

            let left: Vec<u64> = m.tasks().unwrap().iter().map(|t| t.record.id).collect();
            assert_eq!(left, [2]);
            let messages: i64 = m
                .conn
                .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
                .unwrap();
            assert_eq!(messages, 0);
            assert_eq!(events(&m).last().unwrap(), &(1, "deleted".to_string()));
            assert!(m.task_events_intact().unwrap());
        }
        let bytes = std::fs::read(&path).unwrap().to_ascii_lowercase();
        for word in ["quokkabridge", "zanzibarnia"] {
            assert!(
                !bytes.windows(word.len()).any(|w| w == word.as_bytes()),
                "{word} is still in the file"
            );
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn only_finished_tasks_past_their_time_are_erased() {
        let mut m = SqliteMemory::open_in_memory().unwrap();
        let waiting = TaskState::Waiting(WaitingCall {
            tool: "send_email".to_string(),
            arguments: "{}".to_string(),
            until: Timestamp(60),
        });
        m.save_task(&task(1, TaskState::Done, 10)).unwrap();
        m.save_task(&task(2, TaskState::Expired, 20)).unwrap();
        m.save_task(&task(3, TaskState::Failed("stopped".to_string()), 30))
            .unwrap();
        m.save_task(&task(4, waiting, 40)).unwrap();
        m.save_task(&task(5, TaskState::Working, 50)).unwrap();
        m.save_task(&task(6, TaskState::Done, 500)).unwrap();
        m.add_said(1, Timestamp(10), &Said::User("old".to_string()))
            .unwrap();

        assert_eq!(
            m.erase_tasks_finished_before(Timestamp(100), Timestamp(1000))
                .unwrap(),
            [1, 2]
        );
        let left: Vec<u64> = m.tasks().unwrap().iter().map(|t| t.record.id).collect();
        assert_eq!(left, [3, 4, 5, 6]);
        let erased: Vec<(i64, String)> = events(&m)
            .into_iter()
            .filter(|(_, event)| event == "erased")
            .collect();
        assert_eq!(
            erased,
            [(1, "erased".to_string()), (2, "erased".to_string())]
        );
    }

    #[test]
    fn a_task_s_events_each_check_against_the_hash_before_them() {
        let mut m = SqliteMemory::open_in_memory().unwrap();
        m.save_task(&task(1, TaskState::Working, 10)).unwrap();
        m.save_task(&TaskRecord {
            parent: Some(1),
            ..task(2, TaskState::Working, 20)
        })
        .unwrap();
        m.save_task(&task(1, TaskState::Done, 30)).unwrap();
        m.save_task(&task(2, TaskState::Done, 40)).unwrap();

        let events = m.task_events(2).unwrap();
        let names: Vec<&str> = events.iter().map(|e| e.event.as_str()).collect();
        assert_eq!(names, ["opened", "done"]);
        assert_eq!(events[0].parent, Some(1));
        // Chained to task 1's events in between, and each checks on its own.
        assert_eq!(events[0].previous, m.task_events(1).unwrap()[0].hash);
        assert_eq!(events[1].previous, m.task_events(1).unwrap()[1].hash);
        for e in &events {
            let parent = e.parent.map(|p| p as i64);
            let hash = event_hash(&e.previous, 2, e.at.0 as i64, &e.event, parent);
            assert_eq!(hash, e.hash);
        }
        assert_eq!(m.task_events(1).unwrap()[0].previous, "");
    }

    #[test]
    fn an_edited_or_missing_event_breaks_the_chain() {
        let mut m = SqliteMemory::open_in_memory().unwrap();
        for id in 1..=3 {
            m.save_task(&task(id, TaskState::Working, id * 10)).unwrap();
            m.save_task(&task(id, TaskState::Done, id * 10 + 1))
                .unwrap();
        }
        assert!(m.task_events_intact().unwrap());
        m.conn
            .execute("UPDATE task_events SET at = 999 WHERE id = 2", [])
            .unwrap();
        assert!(!m.task_events_intact().unwrap());
        m.conn
            .execute("UPDATE task_events SET at = 11 WHERE id = 2", [])
            .unwrap();
        assert!(m.task_events_intact().unwrap());
        m.conn
            .execute("DELETE FROM task_events WHERE id = 3", [])
            .unwrap();
        assert!(!m.task_events_intact().unwrap());
    }
}
