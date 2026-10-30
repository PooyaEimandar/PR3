//! Tasks: the user's goals, each with its own conversation, so a follow-up continues the
//! task it belongs to and a new topic starts fresh (docs Sec. 4.8). Deterministic rules,
//! not the model, decide which task a message belongs to and which page it shows on.
//! Each task is its own core session, so its history is its own. With the memory
//! database, each change is written through to it, so tasks, their chats and their
//! events survive restarts; without it they last until the app closes.

use std::sync::{Arc, Mutex};

use pr3_core::{Message, Role, SessionId, Timestamp, ToolCall};
use pr3_memory_sqlite::{Said, SqliteMemory, StoredTask, TaskRecord, TaskState, WaitingCall};
use serde::Serialize;
use tauri::ipc::Channel;

use crate::host::{Host, TurnEvent};

mod word;

/// A message this long after its task's last one opens a new task: 10 quiet minutes.
pub const FOLLOW_UP_MS: u64 = 10 * 60 * 1000;
const DAY_MS: u64 = 24 * 60 * 60 * 1000;
/// Why a task that was working when the app closed did not finish.
const CLOSED: &str = "PR3 closed before it finished";
/// The outcome of a call whose confirmation card ran out while the app was closed.
const NOT_ASKED_AGAIN: &str = "NOT SENT: the confirmation ran out while PR3 was closed.";
/// Longest task title, in characters, cut from the message that opened it.
const TITLE_CHARS: usize = 48;
/// Longest plain line under a title, in characters.
const LINE_CHARS: usize = 90;

/// Which of the three pages a task shows on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Page {
    /// Waits for the user, or did not finish.
    Todo,
    /// PR3 is on it: the task the conversation is on.
    Doing,
    Done,
}

/// One step of a task's chat, as the conversation showed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Entry {
    User {
        text: String,
    },
    Reply {
        text: String,
    },
    Tool {
        name: String,
        arguments: String,
        success: bool,
        output: String,
    },
    Failed {
        reason: String,
    },
}

/// A task as the list shows it: casual words, never states or ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskCard {
    pub id: u64,
    pub title: String,
    pub page: Page,
    /// What it waits on, or how it ended.
    pub line: String,
    /// Milliseconds since the epoch.
    pub updated_at: u64,
}

/// A task opened from the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskView {
    #[serde(flatten)]
    pub card: TaskCard,
    /// The task it follows up, if that is still kept.
    pub parent: Option<Link>,
    /// A turn is running for it.
    pub running: bool,
    /// Its confirmation card waits for the user; after a restart, ask again.
    pub waiting: bool,
    pub chat: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Link {
    pub id: u64,
    pub title: String,
}

/// A message opened a new task. The task the conversation was on, if any, is set aside,
/// and the UI folds it into one line before the reply streams.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Opened {
    pub id: u64,
    pub previous: Option<TaskCard>,
}

/// A task the conversation goes back to, and the one it sets aside.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Continued {
    pub task: TaskView,
    pub previous: Option<TaskCard>,
}

/// The next message follows up `parent`; `previous` is set aside meanwhile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FollowingUp {
    pub parent: TaskCard,
    pub previous: Option<TaskCard>,
}

/// Words about the task list, answered without the model (docs Sec. 4.8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Answer {
    /// What is still open: the To Do page, newest first.
    Open { cards: Vec<TaskCard> },
    /// The one unfinished task the words name is the conversation's again.
    Continued(Continued),
    /// Only finished tasks match: the next message follows up the newest.
    FollowingUp(FollowingUp),
    /// Several unfinished tasks match; the user picks one.
    Choose { cards: Vec<TaskCard> },
    /// How the task the words name stands.
    Status { card: TaskCard },
}

/// At most this many tasks to choose between.
const CHOICES: usize = 4;

#[derive(Debug)]
struct Task {
    id: SessionId,
    title: String,
    parent: Option<SessionId>,
    updated_at: Timestamp,
    running: bool,
    /// The consequential call a confirmation card waits on.
    waiting: Option<WaitingCall>,
    /// How its last turn ended.
    ending: Ending,
    chat: Vec<(Timestamp, Entry)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Ending {
    Answered,
    /// Why it did not finish.
    Failed(String),
    /// Its confirmation card ran out; nothing was done.
    Expired,
}

#[derive(Default)]
pub struct Tasks {
    /// Oldest first.
    tasks: Vec<Task>,
    /// The task the conversation is on; a message continues it within `FOLLOW_UP_MS`.
    current: Option<SessionId>,
    /// The next message opens a task that follows up this one.
    follows: Option<SessionId>,
    last_id: u64,
    store: Store,
}

impl Tasks {
    /// The tasks kept in `store`, once the finished ones last changed more than
    /// `keep_days` ago are erased. A task that was working when the app closed did not
    /// finish, and one whose confirmation card ran out meanwhile expired; one still
    /// waiting is asked again. The newest is the conversation's task again if it waits
    /// for the user or is within the follow-up window.
    pub fn load(
        store: Arc<Mutex<SqliteMemory>>,
        keep_days: u64,
        now: Timestamp,
    ) -> Result<Self, String> {
        let stored = {
            let mut db = store
                .lock()
                .map_err(|_| "the memory database is unavailable")?;
            let before = Timestamp(now.0.saturating_sub(keep_days.saturating_mul(DAY_MS)));
            db.erase_tasks_finished_before(before, now)
                .map_err(|e| e.to_string())?;
            db.tasks().map_err(|e| e.to_string())?
        };
        let mut tasks = Self {
            store: Store(Some(store)),
            ..Self::default()
        };
        for StoredTask { record, chat } in stored {
            let id = SessionId(record.id);
            let was_working = record.state == TaskState::Working;
            let (waiting, ending) = match record.state {
                TaskState::Waiting(call) => (Some(call), Ending::Answered),
                TaskState::Failed(reason) => (None, Ending::Failed(reason)),
                TaskState::Expired => (None, Ending::Expired),
                TaskState::Done | TaskState::Working => (None, Ending::Answered),
            };
            let ran_out = waiting
                .as_ref()
                .map(|call| call.until)
                .filter(|&until| until <= now);
            tasks.last_id = tasks.last_id.max(record.id);
            tasks.tasks.push(Task {
                id,
                title: record.title,
                parent: record.parent.map(SessionId),
                updated_at: record.updated_at,
                running: false,
                waiting,
                ending,
                chat: chat
                    .into_iter()
                    .map(|(at, said)| (at, said.into()))
                    .collect(),
            });
            // Each ends when it was last seen to.
            if was_working {
                tasks.end(id, Err(CLOSED.to_string()), record.updated_at);
            } else if let Some(until) = ran_out {
                tasks.expire(id, until);
            }
        }
        tasks.current = tasks
            .tasks
            .iter()
            .max_by_key(|t| (t.updated_at, t.id))
            .filter(|t| t.waiting.is_some() || within(t, now))
            .map(|t| t.id);
        Ok(tasks)
    }

    /// Picks the task for `text`, sent at `now`, and starts its turn: the conversation's
    /// task while it is running or within the follow-up window, else a new one.
    pub fn begin(&mut self, text: &str, now: Timestamp) -> (SessionId, Option<Opened>) {
        let continuing = self
            .current
            .filter(|&id| self.get(id).is_some_and(|t| t.running || within(t, now)));
        let (id, opened) = match continuing {
            Some(id) => (id, None),
            None => {
                let previous = self.current;
                // Unique across launches too: trace ids are built from it.
                self.last_id = now.0.max(self.last_id.saturating_add(1));
                let id = SessionId(self.last_id);
                self.tasks.push(Task {
                    id,
                    title: clip(&one_line(text), TITLE_CHARS),
                    parent: self.follows.take(),
                    updated_at: now,
                    running: false,
                    waiting: None,
                    ending: Ending::Answered,
                    chat: Vec::new(),
                });
                self.current = Some(id);
                let previous = previous.and_then(|p| self.card(p, now));
                (id, Some(Opened { id: id.0, previous }))
            }
        };
        if let Some(task) = self.get_mut(id) {
            task.running = true;
            // A call left waiting by an earlier launch gives way to the new message.
            task.waiting = None;
            task.updated_at = now;
        }
        self.save(id);
        self.push(
            id,
            now,
            Entry::User {
                text: text.to_string(),
            },
        );
        (id, opened)
    }

    /// Keeps what a running turn showed the user.
    pub fn record(&mut self, id: SessionId, event: &TurnEvent, now: Timestamp) {
        if let TurnEvent::Tool {
            name,
            arguments,
            success,
            output,
        } = event
        {
            let answered = self
                .get_mut(id)
                .and_then(|task| task.waiting.take())
                .is_some();
            self.push(
                id,
                now,
                Entry::Tool {
                    name: name.clone(),
                    arguments: arguments.clone(),
                    success: *success,
                    output: output.clone(),
                },
            );
            if answered {
                self.save(id);
            }
        }
    }

    /// A consequential call of `id`'s turn waits for the user's OK until `until`.
    pub fn waiting(&mut self, id: SessionId, call: &ToolCall, until: Timestamp, now: Timestamp) {
        if let Some(task) = self.get_mut(id) {
            task.waiting = Some(WaitingCall {
                tool: call.name.clone(),
                arguments: call.arguments.clone(),
                until,
            });
            task.updated_at = now;
        }
        self.save(id);
    }

    /// Starts asking again for the call `id` was waiting on when the app last closed;
    /// the task runs until [`Tasks::end`] or [`Tasks::expire`].
    pub fn resume_waiting(&mut self, id: SessionId) -> Result<WaitingCall, String> {
        self.idle()?;
        let task = self.get_mut(id).ok_or(GONE)?;
        let call = task
            .waiting
            .clone()
            .ok_or("This task is not waiting for your OK.")?;
        task.running = true;
        Ok(call)
    }

    /// Ends a task's turn with its reply, or why it did not finish.
    pub fn end(&mut self, id: SessionId, outcome: Result<String, String>, now: Timestamp) {
        let Some(task) = self.get_mut(id) else {
            return;
        };
        task.running = false;
        task.waiting = None;
        task.updated_at = now;
        let entry = match outcome {
            Ok(text) => {
                task.ending = Ending::Answered;
                Entry::Reply { text }
            }
            Err(reason) => {
                task.ending = Ending::Failed(reason.clone());
                Entry::Failed { reason }
            }
        };
        self.push(id, now, entry);
        self.save(id);
    }

    /// Ends a task's turn without doing anything: its confirmation card ran out.
    pub fn expire(&mut self, id: SessionId, now: Timestamp) {
        let Some(task) = self.get_mut(id) else {
            return;
        };
        task.running = false;
        task.ending = Ending::Expired;
        task.updated_at = now;
        // Not yet in the chat when the app closed while the card was showing.
        if let Some(call) = task.waiting.take() {
            let entry = Entry::Tool {
                name: call.tool,
                arguments: call.arguments,
                success: false,
                output: NOT_ASKED_AGAIN.to_string(),
            };
            self.push(id, now, entry);
        }
        self.save(id);
    }

    /// The task's answered exchanges, oldest first, for the runtime to continue from:
    /// each message the user sent and the reply it got. A message with no reply - one
    /// that failed, or the one being answered now - is left out, and so are tools.
    pub fn history(&self, id: SessionId) -> Vec<Message> {
        let mut messages = Vec::new();
        let mut asked = None;
        for (at, entry) in self.get(id).iter().flat_map(|t| &t.chat) {
            match entry {
                Entry::User { text } => asked = Some((*at, text)),
                Entry::Reply { text } => {
                    if let Some((asked_at, question)) = asked.take() {
                        messages.push(Message::new(Role::User, question.clone(), asked_at));
                        messages.push(Message::new(Role::Assistant, text.clone(), *at));
                    }
                }
                Entry::Failed { .. } => asked = None,
                Entry::Tool { .. } => {}
            }
        }
        messages
    }

    /// Every task, newest first.
    pub fn cards(&self, now: Timestamp) -> Vec<TaskCard> {
        let mut cards: Vec<TaskCard> = self.tasks.iter().map(|t| self.card_of(t, now)).collect();
        cards.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(b.id.cmp(&a.id)));
        cards
    }

    pub fn view(&self, id: SessionId, now: Timestamp) -> Option<TaskView> {
        let task = self.get(id)?;
        Some(TaskView {
            card: self.card_of(task, now),
            parent: task.parent.and_then(|p| self.get(p)).map(|p| Link {
                id: p.id.0,
                title: p.title.clone(),
            }),
            running: task.running,
            waiting: task.waiting.is_some(),
            chat: task.chat.iter().map(|(_, entry)| entry.clone()).collect(),
        })
    }

    /// The conversation's task, while it runs, waits for the user or is within the
    /// follow-up window: what the conversation shows when the app opens.
    pub fn current(&self, now: Timestamp) -> Option<TaskView> {
        let id = self.current?;
        let task = self.get(id)?;
        let shown = task.running || task.waiting.is_some() || within(task, now);
        shown.then(|| self.view(id, now)).flatten()
    }

    /// Sets the conversation's task aside, so the next message opens a new one.
    pub fn new_topic(&mut self, now: Timestamp) -> Result<Option<TaskCard>, String> {
        self.idle()?;
        self.follows = None;
        Ok(self.set_aside(None, now))
    }

    /// Makes an unfinished task the conversation's task again.
    pub fn continue_task(&mut self, id: SessionId, now: Timestamp) -> Result<Continued, String> {
        self.idle()?;
        let task = self.view(id, now).ok_or(GONE)?;
        if task.card.page == Page::Done {
            return Err("This task is done; follow it up instead.".to_string());
        }
        self.follows = None;
        let previous = self.set_aside(Some(id), now);
        if let Some(task) = self.get_mut(id) {
            // Continuing opens the follow-up window again.
            task.updated_at = now;
        }
        self.save(id);
        let task = self.view(id, now).ok_or(GONE)?;
        Ok(Continued { task, previous })
    }

    /// The next message opens a new task that follows up `id`.
    pub fn follow_up(&mut self, id: SessionId, now: Timestamp) -> Result<FollowingUp, String> {
        self.idle()?;
        let previous = self.set_aside(None, now);
        let parent = self.card(id, now).ok_or(GONE)?;
        self.follows = Some(id);
        Ok(FollowingUp { parent, previous })
    }

    /// Answers a whole message about the task list - "what is still open?", "carry on
    /// with the email to Alex", "what happened with the dentist?" - or returns `None`
    /// when it is not one, or names no kept task, and goes to the model as usual.
    pub fn answer(&mut self, text: &str, now: Timestamp) -> Result<Option<Answer>, String> {
        let Some(words) = word::parse(text) else {
            return Ok(None);
        };
        match words {
            word::Words::StillOpen => {
                let cards = self.cards(now);
                let cards = cards.into_iter().filter(|c| c.page == Page::Todo);
                Ok(Some(Answer::Open {
                    cards: cards.collect(),
                }))
            }
            word::Words::WhatHappened(name) => Ok(self
                .named(&name, now)
                .into_iter()
                .next()
                .map(|card| Answer::Status { card })),
            word::Words::CarryOn(name) => {
                let named = self.named(&name, now);
                let mut open: Vec<TaskCard> = named
                    .iter()
                    .filter(|c| c.page != Page::Done)
                    .cloned()
                    .collect();
                match (open.len(), named.first()) {
                    (0, None) => Ok(None),
                    (0, Some(done)) => {
                        let id = SessionId(done.id);
                        Ok(Some(Answer::FollowingUp(self.follow_up(id, now)?)))
                    }
                    (1, _) => {
                        let id = SessionId(open[0].id);
                        Ok(Some(Answer::Continued(self.continue_task(id, now)?)))
                    }
                    _ => {
                        open.truncate(CHOICES);
                        Ok(Some(Answer::Choose { cards: open }))
                    }
                }
            }
        }
    }

    /// The tasks whose title or opening message holds every word of `name`, newest
    /// first.
    fn named(&self, name: &[String], now: Timestamp) -> Vec<TaskCard> {
        let mut cards: Vec<TaskCard> = self
            .tasks
            .iter()
            .filter(|task| {
                let opened = task.chat.iter().find_map(|(_, entry)| match entry {
                    Entry::User { text } => Some(text.as_str()),
                    _ => None,
                });
                let said = word::normalize(&format!("{} {}", task.title, opened.unwrap_or("")));
                let words: Vec<&str> = said.split(' ').collect();
                word::covers(&words, name)
            })
            .map(|task| self.card_of(task, now))
            .collect();
        cards.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(b.id.cmp(&a.id)));
        cards
    }

    /// Whether `id` can be deleted now: it exists and no turn runs for it.
    pub fn deletable(&self, id: SessionId) -> Result<(), String> {
        match self.get(id) {
            None => Err(GONE.to_string()),
            Some(task) if task.running => Err("PR3 is still answering in this task.".to_string()),
            Some(_) => Ok(()),
        }
    }

    pub fn delete(&mut self, id: SessionId, now: Timestamp) -> Result<(), String> {
        self.deletable(id)?;
        self.store.delete(id, now)?;
        self.tasks.retain(|t| t.id != id);
        if self.current == Some(id) {
            self.current = None;
        }
        if self.follows == Some(id) {
            self.follows = None;
        }
        Ok(())
    }

    /// Refuses to switch tasks while PR3 is answering: the conversation on screen, and
    /// any confirmation card in it, belong to the running task.
    fn idle(&self) -> Result<(), String> {
        if self.tasks.iter().any(|t| t.running) {
            return Err("PR3 is still answering; try again when it is done.".to_string());
        }
        Ok(())
    }

    /// Makes `next` the conversation's task, returning the one it replaces, if any.
    fn set_aside(&mut self, next: Option<SessionId>, now: Timestamp) -> Option<TaskCard> {
        let previous = std::mem::replace(&mut self.current, next).filter(|&p| Some(p) != next);
        previous.and_then(|p| self.card(p, now))
    }

    fn card(&self, id: SessionId, now: Timestamp) -> Option<TaskCard> {
        self.get(id).map(|t| self.card_of(t, now))
    }

    fn card_of(&self, task: &Task, now: Timestamp) -> TaskCard {
        let current = self.current == Some(task.id) && (task.running || within(task, now));
        let (page, line) = if task.waiting.is_some() {
            (Page::Todo, "Waiting for your OK".to_string())
        } else if task.running {
            (Page::Doing, "Working on it…".to_string())
        } else if current {
            (Page::Doing, "In the conversation now".to_string())
        } else {
            match &task.ending {
                Ending::Failed(reason) => {
                    (Page::Todo, format!("Didn't finish: {}", one_line(reason)))
                }
                Ending::Expired => (Page::Done, "Expired: nothing was sent".to_string()),
                Ending::Answered => {
                    let reply = task.chat.iter().rev().find_map(|(_, e)| match e {
                        Entry::Reply { text } if !text.trim().is_empty() => Some(one_line(text)),
                        _ => None,
                    });
                    (Page::Done, reply.unwrap_or_else(|| "Done".to_string()))
                }
            }
        };
        TaskCard {
            id: task.id.0,
            title: task.title.clone(),
            page,
            line: clip(&line, LINE_CHARS),
            updated_at: task.updated_at.0,
        }
    }

    fn get(&self, id: SessionId) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    fn get_mut(&mut self, id: SessionId) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    /// Adds a step to a task's chat.
    fn push(&mut self, id: SessionId, at: Timestamp, entry: Entry) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            self.store.add(id, at, &entry);
            task.chat.push((at, entry));
        }
    }

    /// Writes a task's record through to the store.
    fn save(&self, id: SessionId) {
        if let Some(task) = self.get(id) {
            self.store.save(task);
        }
    }
}

const GONE: &str = "This task is no longer kept.";

/// Where tasks are kept across launches, if anywhere. A failed write is logged and the
/// conversation goes on: the task is still on screen, only not kept.
#[derive(Default)]
struct Store(Option<Arc<Mutex<SqliteMemory>>>);

impl Store {
    fn save(&self, task: &Task) {
        let state = match (&task.waiting, task.running, &task.ending) {
            (Some(call), _, _) => TaskState::Waiting(call.clone()),
            (None, true, _) => TaskState::Working,
            (None, false, Ending::Answered) => TaskState::Done,
            (None, false, Ending::Failed(reason)) => TaskState::Failed(reason.clone()),
            (None, false, Ending::Expired) => TaskState::Expired,
        };
        let record = TaskRecord {
            id: task.id.0,
            title: task.title.clone(),
            parent: task.parent.map(|p| p.0),
            state,
            updated_at: task.updated_at,
        };
        self.write(task.id, |db| db.save_task(&record));
    }

    fn add(&self, id: SessionId, at: Timestamp, entry: &Entry) {
        let said = Said::from(entry.clone());
        self.write(id, |db| db.add_said(id.0, at, &said));
    }

    /// Deleting must reach the store: the user asked for the words to be gone.
    fn delete(&self, id: SessionId, now: Timestamp) -> Result<(), String> {
        let Some(store) = &self.0 else {
            return Ok(());
        };
        let mut db = store
            .lock()
            .map_err(|_| "the memory database is unavailable")?;
        db.delete_task(id.0, now)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn write(
        &self,
        id: SessionId,
        write: impl FnOnce(&mut SqliteMemory) -> Result<(), pr3_memory::MemoryError>,
    ) {
        let Some(store) = &self.0 else {
            return;
        };
        let written = match store.lock() {
            Ok(mut db) => write(&mut db).map_err(|e| e.0),
            Err(_) => Err("the memory database is unavailable".to_string()),
        };
        if let Err(e) = written {
            eprintln!("PR3: task {} is not kept: {e}", id.0);
        }
    }
}

impl From<Entry> for Said {
    fn from(entry: Entry) -> Self {
        match entry {
            Entry::User { text } => Self::User(text),
            Entry::Reply { text } => Self::Reply(text),
            Entry::Tool {
                name,
                arguments,
                success,
                output,
            } => Self::Tool {
                name,
                arguments,
                success,
                output,
            },
            Entry::Failed { reason } => Self::Failed(reason),
        }
    }
}

impl From<Said> for Entry {
    fn from(said: Said) -> Self {
        match said {
            Said::User(text) => Self::User { text },
            Said::Reply(text) => Self::Reply { text },
            Said::Tool {
                name,
                arguments,
                success,
                output,
            } => Self::Tool {
                name,
                arguments,
                success,
                output,
            },
            Said::Failed(reason) => Self::Failed { reason },
        }
    }
}

fn within(task: &Task, now: Timestamp) -> bool {
    now.0.saturating_sub(task.updated_at.0) <= FOLLOW_UP_MS
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max.saturating_sub(1)).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('…');
    cut
}

/// The kept tasks, newest first.
#[tauri::command]
pub fn list_tasks(host: tauri::State<'_, Host>) -> Result<Vec<TaskCard>, String> {
    host.task_cards()
}

/// A task with its chat.
#[tauri::command]
pub fn open_task(id: u64, host: tauri::State<'_, Host>) -> Result<TaskView, String> {
    host.task(SessionId(id))
}

/// The task the conversation is on, if any: what it shows when the app opens.
#[tauri::command]
pub fn current_task(host: tauri::State<'_, Host>) -> Result<Option<TaskView>, String> {
    host.current_task()
}

/// Shows again the confirmation card a task was waiting on when the app closed;
/// returns the line to show as PR3's answer.
#[tauri::command]
pub async fn ask_again(
    id: u64,
    on_event: Channel<TurnEvent>,
    host: tauri::State<'_, Host>,
) -> Result<String, String> {
    host.ask_again(SessionId(id), move |event| {
        // A closed channel only means the window went away.
        let _ = on_event.send(event);
    })
    .await
}

/// Sets the conversation's task aside; returns it, to fold into one line.
#[tauri::command]
pub fn new_topic(host: tauri::State<'_, Host>) -> Result<Option<TaskCard>, String> {
    host.new_topic()
}

#[tauri::command]
pub fn continue_task(id: u64, host: tauri::State<'_, Host>) -> Result<Continued, String> {
    host.continue_task(SessionId(id))
}

#[tauri::command]
pub fn follow_up_task(id: u64, host: tauri::State<'_, Host>) -> Result<FollowingUp, String> {
    host.follow_up(SessionId(id))
}

/// Answers a message about the task list, or `None` for one the model should answer.
#[tauri::command]
pub fn task_words(text: String, host: tauri::State<'_, Host>) -> Result<Option<Answer>, String> {
    host.task_words(&text)
}

/// Deletes a task, its chat and what the runtime remembers of it.
#[tauri::command]
pub fn delete_task(id: u64, host: tauri::State<'_, Host>) -> Result<(), String> {
    host.delete_task(SessionId(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: Timestamp = Timestamp(1_000_000);

    fn at(minutes: u64) -> Timestamp {
        Timestamp(T0.0 + minutes * 60 * 1000)
    }

    /// Sends `text` at `now` and ends its turn with `reply`.
    fn say(
        tasks: &mut Tasks,
        text: &str,
        now: Timestamp,
        reply: &str,
    ) -> (SessionId, Option<Opened>) {
        let (id, opened) = tasks.begin(text, now);
        tasks.end(id, Ok(reply.to_string()), now);
        (id, opened)
    }

    fn page(tasks: &Tasks, id: SessionId, now: Timestamp) -> (Page, String) {
        let card = tasks.card(id, now).unwrap();
        (card.page, card.line)
    }

    fn email() -> ToolCall {
        ToolCall {
            id: "c1".to_string(),
            name: "send_email".to_string(),
            arguments: r#"{"to":["Alex"],"subject":"Q3"}"#.to_string(),
        }
    }

    fn declined() -> TurnEvent {
        TurnEvent::Tool {
            name: "send_email".to_string(),
            arguments: "{}".to_string(),
            success: false,
            output: "declined".to_string(),
        }
    }

    fn store() -> Arc<Mutex<SqliteMemory>> {
        Arc::new(Mutex::new(SqliteMemory::open_in_memory().unwrap()))
    }

    fn texts(chat: &[Entry]) -> Vec<String> {
        chat.iter()
            .map(|e| match e {
                Entry::User { text } => format!("user: {text}"),
                Entry::Reply { text } => format!("reply: {text}"),
                Entry::Tool { name, output, .. } => format!("{name}: {output}"),
                Entry::Failed { reason } => format!("failed: {reason}"),
            })
            .collect()
    }

    #[test]
    fn tasks_and_their_chats_survive_a_restart() {
        let store = store();
        let mut tasks = Tasks::load(Arc::clone(&store), 30, at(0)).unwrap();
        let (first, _) = say(&mut tasks, "What is 2+2?", at(0), "4");
        tasks.follow_up(first, at(20)).unwrap();
        let (child, _) = tasks.begin("And 3+3?", at(21));
        tasks.record(
            child,
            &TurnEvent::Tool {
                name: "calculate".to_string(),
                arguments: "{}".to_string(),
                success: true,
                output: "6".to_string(),
            },
            at(21),
        );
        tasks.end(child, Ok("6".to_string()), at(21));
        drop(tasks);

        // Within the follow-up window: the conversation is back on the last task, and a
        // message continues it.
        let mut tasks = Tasks::load(Arc::clone(&store), 30, at(25)).unwrap();
        assert_eq!(
            tasks.cards(at(25)).iter().map(|c| c.id).collect::<Vec<_>>(),
            [child.0, first.0]
        );
        let current = tasks.current(at(25)).unwrap();
        assert_eq!(current.card.id, child.0);
        assert_eq!(current.parent.unwrap().id, first.0);
        assert_eq!(
            texts(&current.chat),
            ["user: And 3+3?", "calculate: 6", "reply: 6"]
        );
        assert_eq!(tasks.begin("And 4+4?", at(26)).0, child);
        tasks.end(child, Ok("8".to_string()), at(26));
        // A new task's id comes after every kept one.
        tasks.new_topic(at(27)).unwrap();
        assert!(tasks.begin("next", at(0)).0 > child);
        drop(tasks);

        // Later, nothing is the conversation's task.
        let tasks = Tasks::load(store, 30, at(60)).unwrap();
        assert!(tasks.current(at(60)).is_none());
        assert_eq!(tasks.cards(at(60)).len(), 3);
    }

    #[test]
    fn a_turn_cut_off_by_closing_did_not_finish() {
        let store = store();
        let mut tasks = Tasks::load(Arc::clone(&store), 30, at(0)).unwrap();
        let (id, _) = tasks.begin("Plan the trip", at(0));
        drop(tasks);

        let tasks = Tasks::load(store, 30, at(30)).unwrap();
        assert_eq!(
            page(&tasks, id, at(30)),
            (Page::Todo, format!("Didn't finish: {CLOSED}"))
        );
        assert_eq!(
            texts(&tasks.view(id, at(30)).unwrap().chat),
            [
                "user: Plan the trip".to_string(),
                format!("failed: {CLOSED}")
            ]
        );
    }

    #[test]
    fn a_waiting_card_is_asked_again_or_has_expired() {
        let store = store();
        let mut tasks = Tasks::load(Arc::clone(&store), 30, at(0)).unwrap();
        let (id, _) = tasks.begin("Email Alex the Q3 summary", at(0));
        tasks.waiting(id, &email(), at(2), at(0));
        drop(tasks);

        // Back before the card ran out: still waiting, and the conversation's task.
        let mut tasks = Tasks::load(Arc::clone(&store), 30, at(1)).unwrap();
        let current = tasks.current(at(1)).unwrap();
        assert!(current.waiting);
        assert_eq!(current.card.line, "Waiting for your OK");
        let call = tasks.resume_waiting(id).unwrap();
        assert_eq!(
            (call.tool.as_str(), call.arguments),
            ("send_email", email().arguments)
        );
        // Asked again: a new card with a new deadline, and still not answered.
        tasks.waiting(id, &email(), at(3), at(1));
        assert!(tasks.resume_waiting(id).is_err());
        drop(tasks);

        // Back after it ran out: nothing was sent, the conversation shows so, and the
        // task is done.
        let tasks = Tasks::load(store, 30, at(4)).unwrap();
        let current = tasks.current(at(4)).unwrap();
        assert!(!current.waiting);
        assert_eq!(
            page(&tasks, id, at(20)),
            (Page::Done, "Expired: nothing was sent".to_string())
        );
        let chat = texts(&current.chat);
        assert_eq!(
            chat.last().unwrap(),
            &format!("send_email: {NOT_ASKED_AGAIN}")
        );
    }

    #[test]
    fn finished_tasks_are_erased_after_keep_days() {
        let day = |d: u64| Timestamp(T0.0 + d * DAY_MS);
        let store = store();
        let mut tasks = Tasks::load(Arc::clone(&store), 30, day(0)).unwrap();
        let (done, _) = say(&mut tasks, "old question", day(0), "old answer");
        tasks.new_topic(day(0)).unwrap();
        let (failed, _) = tasks.begin("old plan", day(0));
        tasks.end(failed, Err("no model".to_string()), day(0));
        let (recent, _) = say(&mut tasks, "recent question", day(10), "recent answer");
        drop(tasks);

        let tasks = Tasks::load(Arc::clone(&store), 30, day(31)).unwrap();
        let kept: Vec<u64> = tasks.cards(day(31)).iter().map(|c| c.id).collect();
        assert_eq!(kept, [recent.0, failed.0]);
        assert!(tasks.view(done, day(31)).is_none());
        drop(tasks);

        // 0 keeps finished tasks only until the app closes.
        let tasks = Tasks::load(store, 0, day(31)).unwrap();
        let kept: Vec<u64> = tasks.cards(day(31)).iter().map(|c| c.id).collect();
        assert_eq!(kept, [failed.0]);
    }

    #[test]
    fn a_deleted_task_is_gone_from_the_store() {
        let store = store();
        let mut tasks = Tasks::load(Arc::clone(&store), 30, at(0)).unwrap();
        let (id, _) = say(&mut tasks, "secret", at(0), "kept");
        tasks.delete(id, at(1)).unwrap();
        drop(tasks);
        let tasks = Tasks::load(Arc::clone(&store), 30, at(2)).unwrap();
        assert!(tasks.cards(at(2)).is_empty());
        assert!(store.lock().unwrap().task_events_intact().unwrap());
    }

    #[test]
    fn history_is_the_answered_exchanges() {
        let mut tasks = Tasks::default();
        let (id, _) = tasks.begin("Email Alex", at(0));
        tasks.record(id, &declined(), at(0));
        tasks.end(id, Ok("Not sent.".to_string()), at(0));
        tasks.begin("Try again", at(1));
        tasks.end(id, Err("no model".to_string()), at(1));
        tasks.begin("Once more", at(2));
        tasks.end(id, Ok("Sent.".to_string()), at(2));
        // The message being answered now is not history yet.
        tasks.begin("Thanks", at(3));

        let history: Vec<(Role, String)> = tasks
            .history(id)
            .into_iter()
            .map(|m| (m.role, m.content))
            .collect();
        assert_eq!(
            history,
            [
                (Role::User, "Email Alex".to_string()),
                (Role::Assistant, "Not sent.".to_string()),
                (Role::User, "Once more".to_string()),
                (Role::Assistant, "Sent.".to_string()),
            ]
        );
    }

    #[test]
    fn a_follow_up_within_ten_quiet_minutes_continues_the_task() {
        let mut tasks = Tasks::default();
        let (first, opened) = say(
            &mut tasks,
            "What time is it in Tokyo?",
            at(0),
            "It is 9:00.",
        );
        assert_eq!(
            opened,
            Some(Opened {
                id: first.0,
                previous: None
            })
        );
        let (again, opened) = say(&mut tasks, "And in Paris?", at(10), "It is 2:00.");
        assert_eq!((again, opened), (first, None));
        assert_eq!(
            page(&tasks, first, at(10)),
            (Page::Doing, "In the conversation now".to_string())
        );

        // Quiet for longer than the window: the next message opens a new task, and the
        // last one is set aside as done.
        let (next, opened) = say(&mut tasks, "Remind me to call Sam", at(21), "I will.");
        assert_ne!(next, first);
        let previous = opened.unwrap().previous.unwrap();
        assert_eq!((previous.id, previous.page), (first.0, Page::Done));
        assert_eq!(previous.line, "It is 2:00.");
        assert_eq!(tasks.view(first, at(21)).unwrap().chat.len(), 4);
    }

    #[test]
    fn a_new_topic_sets_the_task_aside() {
        let mut tasks = Tasks::default();
        let (first, _) = say(&mut tasks, "hello", at(0), "hi");
        let aside = tasks.new_topic(at(1)).unwrap().unwrap();
        assert_eq!((aside.id, aside.page), (first.0, Page::Done));
        // Already set aside: the next task has nothing to fold.
        let (next, opened) = say(&mut tasks, "something else", at(2), "ok");
        assert_ne!(next, first);
        assert_eq!(opened.unwrap().previous, None);
    }

    #[test]
    fn pages_follow_what_each_task_waits_on() {
        let mut tasks = Tasks::default();
        let (id, _) = tasks.begin("Email Alex the summary", at(0));
        assert_eq!(
            page(&tasks, id, at(0)),
            (Page::Doing, "Working on it…".to_string())
        );

        tasks.waiting(id, &email(), at(2), at(0));
        assert_eq!(
            page(&tasks, id, at(0)),
            (Page::Todo, "Waiting for your OK".to_string())
        );
        tasks.record(id, &declined(), at(1));
        tasks.end(id, Err("the model was stopped".to_string()), at(1));
        // Still the conversation's task, then set aside unfinished.
        assert_eq!(page(&tasks, id, at(1)).0, Page::Doing);
        assert_eq!(
            page(&tasks, id, at(12)),
            (
                Page::Todo,
                "Didn't finish: the model was stopped".to_string()
            )
        );
    }

    #[test]
    fn an_unfinished_task_continues_and_a_done_one_is_followed_up() {
        let mut tasks = Tasks::default();
        let (unfinished, _) = tasks.begin("Plan the trip", at(0));
        tasks.end(unfinished, Err("no model".to_string()), at(0));
        let (done, _) = say(&mut tasks, "What is 2+2?", at(20), "4");

        let continued = tasks.continue_task(unfinished, at(21)).unwrap();
        assert_eq!(continued.task.card.page, Page::Doing);
        assert_eq!(continued.previous.map(|p| p.id), Some(done.0));
        let (same, opened) = tasks.begin("Lisbon, in May", at(22));
        assert_eq!((same, opened), (unfinished, None));
        tasks.end(same, Ok("Here is a plan.".to_string()), at(22));

        assert!(tasks.continue_task(done, at(23)).is_err());
        let following = tasks.follow_up(done, at(23)).unwrap();
        assert_eq!(following.parent.id, done.0);
        assert_eq!(following.previous.map(|p| p.id), Some(unfinished.0));
        let (child, _) = say(&mut tasks, "And 3+3?", at(24), "6");
        let parent = tasks.view(child, at(24)).unwrap().parent.unwrap();
        assert_eq!((parent.id, parent.title.as_str()), (done.0, "What is 2+2?"));
    }

    #[test]
    fn task_words_list_continue_or_follow_up_what_they_name() {
        let mut tasks = Tasks::default();
        let (alex, _) = tasks.begin("Email Alex the Q3 summary", at(0));
        tasks.waiting(alex, &email(), at(2), at(0));
        tasks.end(alex, Err("declined".to_string()), at(1));
        let (trip, _) = tasks.begin("Plan the trip to Lisbon", at(20));
        tasks.end(trip, Err("no model".to_string()), at(20));
        let (sum, _) = say(&mut tasks, "What is 2+2?", at(40), "4");

        // What is still open: the To Do page, newest first.
        let Some(Answer::Open { cards }) = tasks.answer("What's still open?", at(41)).unwrap()
        else {
            panic!("not the open tasks");
        };
        let ids: Vec<u64> = cards.iter().map(|c| c.id).collect();
        assert_eq!(ids, [trip.0, alex.0]);

        // One unfinished match: the conversation goes back to it.
        let Some(Answer::Continued(continued)) = tasks
            .answer("Carry on with the email to Alex", at(42))
            .unwrap()
        else {
            panic!("not continued");
        };
        assert_eq!(continued.task.card.id, alex.0);
        assert_eq!(continued.previous.map(|p| p.id), Some(sum.0));
        let (same, _) = tasks.begin("Send it now", at(43));
        assert_eq!(same, alex);
        tasks.end(same, Ok("Sent to Alex Rivera.".to_string()), at(43));

        // Only a finished match: the next message follows it up.
        let Some(Answer::FollowingUp(following)) =
            tasks.answer("Back to the emails for Alex", at(60)).unwrap()
        else {
            panic!("not followed up");
        };
        assert_eq!(following.parent.id, alex.0);

        // What happened: the newest match, however it stands.
        let Some(Answer::Status { card }) = tasks
            .answer("What happened with the trip?", at(61))
            .unwrap()
        else {
            panic!("no status");
        };
        assert_eq!((card.id, card.page), (trip.0, Page::Todo));
    }

    #[test]
    fn task_words_naming_several_or_none_are_left_to_choose_or_to_the_model() {
        let mut tasks = Tasks::default();
        for (n, text) in ["Email Alex the Q3 summary", "Email Alex about lunch"]
            .iter()
            .enumerate()
        {
            let (id, _) = tasks.begin(text, at(n as u64 * 20));
            tasks.end(id, Err("no model".to_string()), at(n as u64 * 20));
        }
        let Some(Answer::Choose { cards }) = tasks
            .answer("continue with the email to Alex", at(50))
            .unwrap()
        else {
            panic!("nothing to choose");
        };
        assert_eq!(cards.len(), 2);
        // The words can name a task's opening message, not only its title.
        let Some(Answer::Continued(continued)) =
            tasks.answer("carry on with lunch", at(51)).unwrap()
        else {
            panic!("not continued");
        };
        assert_eq!(continued.task.card.title, "Email Alex about lunch");

        for other in [
            "Continue the story",
            "What happened to the Roman empire?",
            "Is the shop still open?",
        ] {
            assert_eq!(tasks.answer(other, at(52)).unwrap(), None, "{other}");
        }
    }

    #[test]
    fn switching_or_deleting_waits_for_the_answer() {
        let mut tasks = Tasks::default();
        let (id, _) = tasks.begin("hello", at(0));
        assert!(tasks.new_topic(at(0)).is_err());
        assert!(tasks.delete(id, at(0)).is_err());
        tasks.end(id, Ok("hi".to_string()), at(0));
        tasks.delete(id, at(0)).unwrap();
        assert!(tasks.cards(at(0)).is_empty());
        // The deleted task was the conversation's: the next message opens a new one.
        let (next, opened) = say(&mut tasks, "again", at(1), "ok");
        assert_ne!(next, id);
        assert_eq!(opened.unwrap().previous, None);
    }

    #[test]
    fn titles_are_one_short_line_and_ids_never_repeat() {
        let mut tasks = Tasks::default();
        let (a, _) = say(
            &mut tasks,
            "  Find the\nreceipt   for the new laptop I bought last spring in Lisbon ",
            at(0),
            "ok",
        );
        tasks.new_topic(at(0)).unwrap();
        let (b, _) = say(&mut tasks, "next", at(0), "ok");
        assert!(b.0 > a.0);
        let title = tasks.view(a, at(0)).unwrap().card.title;
        assert_eq!(title, "Find the receipt for the new laptop I bought la…");
        assert_eq!(title.chars().count(), TITLE_CHARS);
    }
}
