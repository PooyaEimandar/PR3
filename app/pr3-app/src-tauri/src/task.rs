//! Tasks: the user's goals, each with its own conversation, so a follow-up continues the
//! task it belongs to and a new topic starts fresh (docs Sec. 4.8). This is step 1 of
//! that section's build order: tasks live in memory for one launch, and deterministic
//! rules, not the model, decide which task a message belongs to and which page it shows
//! on. Each task is its own core session, so its history is its own.

use pr3_core::{SessionId, Timestamp};
use serde::Serialize;

use crate::host::{Host, TurnEvent};

/// A message this long after its task's last one opens a new task: 10 quiet minutes.
pub const FOLLOW_UP_MS: u64 = 10 * 60 * 1000;
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

#[derive(Debug)]
struct Task {
    id: SessionId,
    title: String,
    parent: Option<SessionId>,
    updated_at: Timestamp,
    running: bool,
    /// A confirmation card waits for the user's answer.
    awaiting_ok: bool,
    /// Why its last turn did not finish, if it did not.
    unfinished: Option<String>,
    chat: Vec<Entry>,
}

#[derive(Debug, Default)]
pub struct Tasks {
    /// Oldest first.
    tasks: Vec<Task>,
    /// The task the conversation is on; a message continues it within `FOLLOW_UP_MS`.
    current: Option<SessionId>,
    /// The next message opens a task that follows up this one.
    follows: Option<SessionId>,
    last_id: u64,
}

impl Tasks {
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
                    awaiting_ok: false,
                    unfinished: None,
                    chat: Vec::new(),
                });
                self.current = Some(id);
                let previous = previous.and_then(|p| self.card(p, now));
                (id, Some(Opened { id: id.0, previous }))
            }
        };
        if let Some(task) = self.get_mut(id) {
            task.running = true;
            task.updated_at = now;
            task.chat.push(Entry::User {
                text: text.to_string(),
            });
        }
        (id, opened)
    }

    /// Keeps what a running turn showed the user.
    pub fn record(&mut self, id: SessionId, event: &TurnEvent) {
        let Some(task) = self.get_mut(id) else {
            return;
        };
        match event {
            TurnEvent::Tool {
                name,
                arguments,
                success,
                output,
            } => {
                task.awaiting_ok = false;
                task.chat.push(Entry::Tool {
                    name: name.clone(),
                    arguments: arguments.clone(),
                    success: *success,
                    output: output.clone(),
                });
            }
            TurnEvent::Approval { .. } => task.awaiting_ok = true,
            TurnEvent::Token { .. } | TurnEvent::TaskOpened(_) => {}
        }
    }

    /// Ends a task's turn with its reply, or why it did not finish.
    pub fn end(&mut self, id: SessionId, outcome: Result<String, String>, now: Timestamp) {
        let Some(task) = self.get_mut(id) else {
            return;
        };
        task.running = false;
        task.awaiting_ok = false;
        task.updated_at = now;
        match outcome {
            Ok(text) => {
                task.unfinished = None;
                task.chat.push(Entry::Reply { text });
            }
            Err(reason) => {
                task.unfinished = Some(reason.clone());
                task.chat.push(Entry::Failed { reason });
            }
        }
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
            chat: task.chat.clone(),
        })
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

    /// Whether `id` can be deleted now: it exists and no turn runs for it.
    pub fn deletable(&self, id: SessionId) -> Result<(), String> {
        match self.get(id) {
            None => Err(GONE.to_string()),
            Some(task) if task.running => Err("PR3 is still answering in this task.".to_string()),
            Some(_) => Ok(()),
        }
    }

    pub fn delete(&mut self, id: SessionId) -> Result<(), String> {
        self.deletable(id)?;
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
        let (page, line) = if task.awaiting_ok {
            (Page::Todo, "Waiting for your OK".to_string())
        } else if task.running {
            (Page::Doing, "Working on it…".to_string())
        } else if current {
            (Page::Doing, "In the conversation now".to_string())
        } else if let Some(reason) = &task.unfinished {
            (Page::Todo, format!("Didn't finish: {}", one_line(reason)))
        } else {
            let reply = task.chat.iter().rev().find_map(|e| match e {
                Entry::Reply { text } if !text.trim().is_empty() => Some(one_line(text)),
                _ => None,
            });
            (Page::Done, reply.unwrap_or_else(|| "Done".to_string()))
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
}

const GONE: &str = "This task is no longer kept.";

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

/// This launch's tasks, newest first.
#[tauri::command]
pub fn list_tasks(host: tauri::State<'_, Host>) -> Result<Vec<TaskCard>, String> {
    host.task_cards()
}

/// A task with its chat.
#[tauri::command]
pub fn open_task(id: u64, host: tauri::State<'_, Host>) -> Result<TaskView, String> {
    host.task(SessionId(id))
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

/// Deletes a task, its chat and what the runtime remembers of it.
#[tauri::command]
pub fn delete_task(id: u64, host: tauri::State<'_, Host>) -> Result<(), String> {
    host.delete_task(SessionId(id))
}

#[cfg(test)]
mod tests {
    use pr3_core::trusted::{ApprovalRequest, Mail, Risk};

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

        let request = ApprovalRequest {
            request: "r1".to_string(),
            operation: "mail.send".to_string(),
            provider: "mail:sandbox".to_string(),
            risk: Risk::Low,
            mail: Mail {
                to: Vec::new(),
                subject: "Q3".to_string(),
                body: String::new(),
            },
            expires_at: at(5),
        };
        tasks.record(id, &TurnEvent::Approval { request });
        assert_eq!(
            page(&tasks, id, at(0)),
            (Page::Todo, "Waiting for your OK".to_string())
        );
        tasks.record(
            id,
            &TurnEvent::Tool {
                name: "send_email".to_string(),
                arguments: "{}".to_string(),
                success: false,
                output: "declined".to_string(),
            },
        );
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
    fn switching_or_deleting_waits_for_the_answer() {
        let mut tasks = Tasks::default();
        let (id, _) = tasks.begin("hello", at(0));
        assert!(tasks.new_topic(at(0)).is_err());
        assert!(tasks.delete(id).is_err());
        tasks.end(id, Ok("hi".to_string()), at(0));
        tasks.delete(id).unwrap();
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
