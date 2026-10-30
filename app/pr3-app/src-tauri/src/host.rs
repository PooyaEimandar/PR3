//! The host side of the sans-I/O loop: it performs the actions `pr3-core` asks for —
//! running the model, running validated tool calls, storing traces.
//!
//! The core's lock is held only for each `step()`; inference runs on a blocking worker
//! thread with the lock released, so the UI thread never waits on a model.

use std::collections::BTreeMap;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use pr3_core::trusted::ApprovalRequest;
use pr3_core::{
    Action, Config, Core, CoreError, GenerationInfo, InferenceRequest, Input, Message,
    ProposedToolCall, RequestId, Role, SessionId, Timestamp, ToolCall, Trace,
};
use pr3_memory_sqlite::SqliteMemory;
use pr3_tool::{SharedMemory, Tool, ToolContext};

use crate::actions::{self, Actions};
use crate::task::{
    Answer, Continued, Export, Fold, FollowingUp, Opened, TaskCard, TaskView, Tasks,
};
use serde::Serialize;
use serde_json::{Map, Value};

/// Upper bound on host/core round trips per user turn; a backstop, not the normal exit.
/// The core's own limits (tool rounds, loop guard) end a turn long before this.
const MAX_ROUND_TRIPS: usize = 64;

/// Longer messages are not looked up in memory: a long paste is not a question about
/// the user, and the lookup runs before the core checks the message's size.
const MAX_LOOKUP_BYTES: usize = 4 * 1024;

/// Before a turn, a task's older exchanges are folded into its summary once its history
/// is longer than this, about 1,800 tokens: with the system prompt, the tools, the
/// summary and the message, the prompt then stays within the 3,584 tokens that a
/// 4,096-token context leaves beside room for the answer (docs Sec. 4.8).
const FOLD_AT_BYTES: usize = 7 * 1024;
/// Or once it has this many messages, short of the core's limit of 32.
const FOLD_AT_MESSAGES: usize = 24;
/// The latest exchanges kept word for word when the rest are folded.
const KEEP_EXCHANGES: usize = 2;
/// The id of a request for a summary; the runtime's own ids start at 1.
pub const SUMMARY: RequestId = RequestId(0);

/// What a model produced for one inference request.
#[derive(Debug, Clone, Default)]
pub struct Generation {
    pub content: String,
    /// Parsed from the model's output format; validated by the core, not here.
    pub tool_calls: Vec<ProposedToolCall>,
    pub info: GenerationInfo,
}

/// A model the host can run. Host-side and `std`; the core never sees it.
pub trait Backend: Send + Sync {
    /// Short label for the UI, e.g. "mock model" or "Qwen3-0.6B Q8_0".
    fn label(&self) -> String;

    /// Answers `request`, calling `on_token` with each piece of visible text as it is
    /// produced.
    fn generate(
        &self,
        request: &InferenceRequest,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<Generation, String>;

    /// Like `generate`, but `on_token` returns whether to go on; `false` ends this
    /// answer early and leaves the backend ready for the next. For a paired phone that
    /// cancels or goes away. Backends that cannot stop early finish the answer.
    fn generate_until(
        &self,
        request: &InferenceRequest,
        on_token: &mut dyn FnMut(&str) -> bool,
    ) -> Result<Generation, String> {
        self.generate(request, &mut |text| {
            on_token(text);
        })
    }

    /// Ends a generation in progress soon, for shutting down.
    fn stop(&self) {}
}

/// Live events for the UI while a turn runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TurnEvent {
    Token {
        text: String,
    },
    /// A tool the core approved has run; shown so the user sees what PR3 did.
    Tool {
        name: String,
        arguments: String,
        success: bool,
        output: String,
    },
    /// A consequential action waits for the user: show the confirmation card, built
    /// from the validated intent, and send the answer with `answer_approval`.
    Approval {
        request: ApprovalRequest,
    },
    /// The message opened a new task (docs Sec. 4.8); comes before anything else.
    TaskOpened(Opened),
    /// The message seemed to start something new in the task it went to; comes after
    /// the turn ends. The UI offers to move it into a task of its own (`start_new_task`).
    NewTask,
    /// Before the answer, the model summed up a task's earlier exchanges: those of the task
    /// the message follows up, or the oldest of its own (docs Sec. 4.8). What it cost.
    Summary {
        task: u64,
        prompt_tokens: u32,
        completion_tokens: u32,
        ms: u64,
    },
}

pub type TraceSink = Arc<dyn Fn(&Trace) + Send + Sync>;

#[derive(Debug)]
pub enum HostError {
    Core(CoreError),
    /// The core still had work queued after `MAX_ROUND_TRIPS`; the turn is abandoned
    /// rather than silently dropping what was left.
    TooManyRoundTrips,
    /// Something outside the core failed, such as the worker thread.
    Host(String),
}

impl From<CoreError> for HostError {
    fn from(e: CoreError) -> Self {
        Self::Core(e)
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(e) => e.fmt(f),
            Self::TooManyRoundTrips => write!(
                f,
                "this turn needed more than {MAX_ROUND_TRIPS} steps and was stopped"
            ),
            Self::Host(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for HostError {}

pub struct Host {
    core: Mutex<Core>,
    /// Swappable: the app starts with a placeholder and installs the real model once
    /// it has loaded in the background.
    backend: RwLock<Arc<dyn Backend>>,
    tools: BTreeMap<String, Box<dyn Tool>>,
    /// Searched for every message, so the model sees what it knows about the user
    /// without having to ask.
    memory: Option<SharedMemory>,
    /// The trusted path for consequential tools; without it they are refused.
    actions: Option<Arc<Actions>>,
    trace_sink: RwLock<Option<TraceSink>>,
    /// The user's tasks; each is its own core session (docs Sec. 4.8).
    tasks: Arc<Mutex<Tasks>>,
    /// Set once the app is quitting: a model that finishes loading afterwards is
    /// dropped instead of installed.
    shutting_down: AtomicBool,
}

impl Host {
    pub fn new(backend: Arc<dyn Backend>, tools: Vec<Box<dyn Tool>>) -> Result<Self, HostError> {
        let mut core = Core::new(Config::default());
        let mut by_name = BTreeMap::new();
        for tool in tools {
            let spec = tool.spec();
            let name = spec.name.clone();
            core.register_tool(spec)?;
            by_name.insert(name, tool);
        }
        Ok(Self {
            core: Mutex::new(core),
            backend: RwLock::new(backend),
            tools: by_name,
            memory: None,
            actions: None,
            trace_sink: RwLock::new(None),
            shutting_down: AtomicBool::new(false),
            tasks: Arc::new(Mutex::new(Tasks::default())),
        })
    }

    /// Looks up each message in `memory` and gives the matches to the model as notes.
    pub fn with_memory(mut self, memory: SharedMemory) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Keeps the user's tasks in `store`, and brings back the ones it kept, erasing
    /// finished tasks older than `keep_days` (docs Sec. 4.8). Without it, tasks last
    /// until the app closes.
    pub fn with_tasks(mut self, store: Arc<Mutex<SqliteMemory>>, keep_days: u64) -> Self {
        match Tasks::load(store, keep_days, now()) {
            Ok(tasks) => self.tasks = Arc::new(Mutex::new(tasks)),
            Err(e) => eprintln!("PR3: tasks are not kept: {e}"),
        }
        self
    }

    /// Offers the consequential tools of `actions` and runs their calls through it.
    pub fn with_actions(mut self, actions: Arc<Actions>) -> Result<Self, HostError> {
        {
            let mut core = self
                .core
                .lock()
                .map_err(|_| HostError::Host("runtime state is unavailable".to_string()))?;
            for spec in actions.tools() {
                core.register_tool(spec)?;
            }
        }
        self.actions = Some(actions);
        Ok(self)
    }

    /// The user's answer from the confirmation card.
    pub fn answer_approval(&self, request: &str, approve: bool) -> Result<(), String> {
        self.actions
            .as_ref()
            .ok_or("actions are not available")?
            .answer(request, approve)
    }

    pub fn backend_label(&self) -> String {
        self.backend().label()
    }

    pub fn set_backend(&self, backend: Arc<dyn Backend>) {
        if self.shutting_down.load(Ordering::SeqCst) {
            return;
        }
        if let Ok(mut current) = self.backend.write() {
            *current = backend;
        }
    }

    /// For quitting: stops the model's current answer, swaps in a placeholder and
    /// returns the model, so the caller can wait until nothing else holds it and free
    /// it before the process exits. Later models are not installed.
    pub fn shut_down(&self) -> Arc<dyn Backend> {
        self.shutting_down.store(true, Ordering::SeqCst);
        let placeholder: Arc<dyn Backend> = Arc::new(MockBackend::default());
        let model = match self.backend.write() {
            Ok(mut current) => std::mem::replace(&mut *current, placeholder),
            Err(_) => placeholder,
        };
        model.stop();
        model
    }

    /// Where finished turns' traces go (for example, an OpenJarvis JSONL file).
    pub fn set_trace_sink(&self, sink: TraceSink) {
        if let Ok(mut current) = self.trace_sink.write() {
            *current = Some(sink);
        }
    }

    pub fn task_cards(&self) -> Result<Vec<TaskCard>, String> {
        Ok(self.tasks().map_err(|e| e.to_string())?.cards(now()))
    }

    pub fn task(&self, id: SessionId) -> Result<TaskView, String> {
        self.tasks()
            .map_err(|e| e.to_string())?
            .view(id, now())
            .ok_or_else(|| "This task is no longer kept.".to_string())
    }

    pub fn export_task(&self, id: SessionId, utc_offset_minutes: i32) -> Result<Export, String> {
        self.tasks()
            .map_err(|e| e.to_string())?
            .export(id, now(), utc_offset_minutes)
    }

    pub fn task_words(&self, text: &str) -> Result<Option<Answer>, String> {
        self.tasks().map_err(|e| e.to_string())?.answer(text, now())
    }

    pub fn current_task(&self) -> Result<Option<TaskView>, String> {
        Ok(self.tasks().map_err(|e| e.to_string())?.current(now()))
    }

    /// Moves the last exchange, which seemed to start something new, into a task of its
    /// own (docs Sec. 4.8); the runtime forgets it in the task it came from.
    pub fn start_new_task(&self) -> Result<Opened, String> {
        let (from, opened) = self
            .tasks()
            .map_err(|e| e.to_string())?
            .start_new_task(now())?;
        // Both tasks get their exchanges back from the task list on their next turn.
        self.core
            .lock()
            .map_err(|_| "runtime state is unavailable".to_string())?
            .forget(from)
            .map_err(|e| e.to_string())?;
        Ok(opened)
    }

    /// After a memory is forgotten, which erased every task's summary in the store: drops
    /// them here too, and the runtime's history of the tasks that had one, so their next
    /// turn goes on from their chat.
    pub fn forget_summaries(&self) -> Result<(), String> {
        let summed = self.tasks().map_err(|e| e.to_string())?.forget_summaries();
        let mut core = self.core().map_err(|e| e.to_string())?;
        for id in summed {
            // Refused for a task answering now, which keeps the history it has.
            let _ = core.forget(id);
        }
        Ok(())
    }

    pub fn new_topic(&self) -> Result<Option<TaskCard>, String> {
        self.tasks().map_err(|e| e.to_string())?.new_topic(now())
    }

    pub fn continue_task(&self, id: SessionId) -> Result<Continued, String> {
        self.tasks()
            .map_err(|e| e.to_string())?
            .continue_task(id, now())
    }

    pub fn follow_up(&self, id: SessionId) -> Result<FollowingUp, String> {
        self.tasks()
            .map_err(|e| e.to_string())?
            .follow_up(id, now())
    }

    /// Deletes a task: its chat here, and its history in the core.
    pub fn delete_task(&self, id: SessionId) -> Result<(), String> {
        self.tasks().map_err(|e| e.to_string())?.deletable(id)?;
        self.core
            .lock()
            .map_err(|_| "runtime state is unavailable".to_string())?
            .forget(id)
            .map_err(|e| e.to_string())?;
        self.tasks().map_err(|e| e.to_string())?.delete(id, now())
    }

    /// Asks again for the call a task was waiting on when the app last closed. The
    /// trusted path takes it as a new request, with a new confirmation card streamed
    /// through `on_event`, and the task ends with what came of it; no model runs.
    /// Returns the line to show as PR3's answer.
    pub async fn ask_again<F>(&self, id: SessionId, mut on_event: F) -> Result<String, String>
    where
        F: FnMut(TurnEvent) + Send + 'static,
    {
        let waiting = self
            .tasks()
            .map_err(|e| e.to_string())?
            .resume_waiting(id)?;
        let Some(actions) = &self.actions else {
            let reason = "actions are not available".to_string();
            self.tasks()
                .map_err(|e| e.to_string())?
                .end(id, Err(reason.clone()), now());
            return Err(reason);
        };
        let call = ToolCall {
            id: format!("again-{}", id.0),
            name: waiting.tool,
            arguments: waiting.arguments,
        };
        let mut asked = None;
        let (success, output) = Arc::clone(actions)
            .run(&call, |request| {
                if let Ok(mut tasks) = self.tasks.lock() {
                    tasks.waiting(id, &call, request.expires_at, now());
                }
                asked = Some((actions::names(&request.mail.to), request.expires_at));
                on_event(TurnEvent::Approval {
                    request: request.clone(),
                });
            })
            .await;
        let tool = TurnEvent::Tool {
            name: call.name.clone(),
            arguments: call.arguments.clone(),
            success,
            output: output.clone(),
        };
        on_event(tool.clone());
        let mut tasks = self.tasks().map_err(|e| e.to_string())?;
        tasks.record(id, &tool, now());
        let reply = match asked {
            Some((recipients, _)) if success => format!("Sent to {recipients}."),
            Some((_, until)) if now() >= until => {
                tasks.expire(id, now());
                return Ok("Nothing was sent: the confirmation ran out.".to_string());
            }
            Some(_) => "Nothing was sent.".to_string(),
            // Refused before any card, for example over a limit.
            None => {
                tasks.end(id, Err(output.clone()), now());
                return Err(output);
            }
        };
        tasks.end(id, Ok(reply.clone()), now());
        Ok(reply)
    }

    fn tasks(&self) -> Result<std::sync::MutexGuard<'_, Tasks>, HostError> {
        self.tasks
            .lock()
            .map_err(|_| HostError::Host("the task list is unavailable".to_string()))
    }

    pub(crate) fn backend(&self) -> Arc<dyn Backend> {
        match self.backend.read() {
            Ok(b) => Arc::clone(&b),
            Err(poisoned) => Arc::clone(&poisoned.into_inner()),
        }
    }

    /// Runs one user turn in the task the message belongs to and returns what the UI
    /// should show at the end (a reply or a failure). Tokens and tool activity stream
    /// through `on_event` meanwhile, after `TaskOpened` if the message opened a task.
    pub async fn send<F>(
        &self,
        text: String,
        utc_offset_minutes: i32,
        mut on_event: F,
    ) -> Result<Vec<Action>, HostError>
    where
        F: FnMut(TurnEvent) + Send + 'static,
    {
        let (session, opened) = self.tasks()?.begin(&text, now());
        if let Some(opened) = opened {
            on_event(TurnEvent::TaskOpened(opened));
        }
        let on_event = Arc::new(Mutex::new(on_event));
        // The task keeps its chat as the user saw it.
        let tasks = Arc::clone(&self.tasks);
        let emit = Arc::clone(&on_event);
        let recorded = move |event: TurnEvent| {
            if let Ok(mut tasks) = tasks.lock() {
                tasks.record(session, &event, now());
            }
            if let Ok(mut emit) = emit.lock() {
                emit(event);
            }
        };
        let result = self.turn(session, text, utc_offset_minutes, recorded).await;
        let outcome = match &result {
            Ok(actions) => actions
                .iter()
                .find_map(|action| match action {
                    Action::Reply { content, .. } => Some(Ok(content.clone())),
                    Action::Failed { reason, .. } => Some(Err(reason.clone())),
                    _ => None,
                })
                .unwrap_or_else(|| Err("the turn ended without an answer".to_string())),
            Err(error) => Err(error.to_string()),
        };
        let seems_new = {
            let mut tasks = self.tasks()?;
            tasks.end(session, outcome, now());
            tasks.seems_new(session)
        };
        if seems_new && let Ok(mut emit) = on_event.lock() {
            emit(TurnEvent::NewTask);
        }
        result
    }

    async fn turn<F>(
        &self,
        session: SessionId,
        text: String,
        utc_offset_minutes: i32,
        on_event: F,
    ) -> Result<Vec<Action>, HostError>
    where
        F: FnMut(TurnEvent) + Send + 'static,
    {
        self.restore(session)?;
        let on_event = Arc::new(Mutex::new(on_event));
        self.catch_up(session, &on_event).await?;
        let utterance = text.clone();
        let notes = match &self.memory {
            Some(memory) if text.len() <= MAX_LOOKUP_BYTES => {
                pr3_tool::notes_for(memory, &text, now())
            }
            _ => Vec::new(),
        };
        // If the core refuses the message, no turn was started.
        let queue = self.step(Input::Utterance {
            session,
            content: text,
            notes,
            at: now(),
        })?;
        match self
            .drive(session, queue, utc_offset_minutes, &utterance, on_event)
            .await
        {
            Ok(for_ui) => Ok(for_ui),
            // The turn is still open: end it, so the session takes the next message.
            Err(error) => {
                let actions = self.step(Input::Abort {
                    session,
                    reason: error.to_string(),
                    at: now(),
                })?;
                Ok(self.deliver(actions))
            }
        }
    }

    /// Performs the core's actions until the turn ends.
    async fn drive<F>(
        &self,
        session: SessionId,
        mut queue: Vec<Action>,
        utc_offset_minutes: i32,
        utterance: &str,
        on_event: Arc<Mutex<F>>,
    ) -> Result<Vec<Action>, HostError>
    where
        F: FnMut(TurnEvent) + Send + 'static,
    {
        let mut for_ui = Vec::new();
        for _ in 0..MAX_ROUND_TRIPS {
            if queue.is_empty() {
                break;
            }
            match queue.remove(0) {
                Action::Infer(request) => {
                    let id = request.id;
                    let input = match self.infer(request, Arc::clone(&on_event)).await {
                        Ok(Ok(g)) => Input::ModelOutput {
                            request: id,
                            content: g.content,
                            tool_calls: g.tool_calls,
                            info: g.info,
                            at: now(),
                        },
                        Ok(Err(reason)) => Input::ModelFailed {
                            request: id,
                            reason,
                            at: now(),
                        },
                        // The worker itself failed (a panic): report it like a model error.
                        Err(error) => Input::ModelFailed {
                            request: id,
                            reason: error.to_string(),
                            at: now(),
                        },
                    };
                    queue.extend(self.step(input)?);
                }
                Action::CallTool(call) => {
                    let ctx = ToolContext {
                        now: now(),
                        utc_offset_minutes,
                        session,
                        utterance: utterance.to_string(),
                    };
                    let (success, output) = self.run_tool(&call, &ctx);
                    if let Ok(mut emit) = on_event.lock() {
                        emit(TurnEvent::Tool {
                            name: call.name.clone(),
                            arguments: call.arguments.clone(),
                            success,
                            output: output.clone(),
                        });
                    }
                    queue.extend(self.step(Input::ToolOutcome {
                        call_id: call.id,
                        success,
                        output,
                        at: now(),
                    })?);
                }
                Action::Propose(call) => {
                    let (success, output) = match &self.actions {
                        Some(actions) => {
                            let ask = |request: &ApprovalRequest| {
                                // Kept, so the card is asked again if the app closes.
                                if let Ok(mut tasks) = self.tasks.lock() {
                                    tasks.waiting(session, &call, request.expires_at, now());
                                }
                                if let Ok(mut emit) = on_event.lock() {
                                    emit(TurnEvent::Approval {
                                        request: request.clone(),
                                    });
                                }
                            };
                            Arc::clone(actions).run(&call, ask).await
                        }
                        None => (false, "actions are not available".to_string()),
                    };
                    if let Ok(mut emit) = on_event.lock() {
                        emit(TurnEvent::Tool {
                            name: call.name.clone(),
                            arguments: call.arguments.clone(),
                            success,
                            output: output.clone(),
                        });
                    }
                    queue.extend(self.step(Input::ToolOutcome {
                        call_id: call.id,
                        success,
                        output,
                        at: now(),
                    })?);
                }
                Action::Trace(trace) => self.record(&trace),
                other => for_ui.push(other),
            }
        }
        if !queue.is_empty() {
            return Err(HostError::TooManyRoundTrips);
        }
        Ok(for_ui)
    }

    /// Records traces and returns what the UI should show.
    fn deliver(&self, actions: Vec<Action>) -> Vec<Action> {
        actions
            .into_iter()
            .filter(|a| match a {
                Action::Trace(trace) => {
                    self.record(trace);
                    false
                }
                _ => true,
            })
            .collect()
    }

    fn step(&self, input: Input) -> Result<Vec<Action>, HostError> {
        Ok(self.core()?.step(input)?)
    }

    /// A task kept from an earlier launch gets its exchanges back in the runtime before
    /// its next turn, so a follow-up keeps its context across restarts.
    fn restore(&self, session: SessionId) -> Result<(), HostError> {
        if self.core()?.history(session).next().is_some() {
            return Ok(());
        }
        let earlier = self.tasks()?.history(session);
        if !earlier.is_empty() {
            self.core()?.restore(session, earlier)?;
        }
        Ok(())
    }

    /// Before a turn (docs Sec. 4.8): sums up the task `session` follows up, as far as its
    /// summary does not cover its chat yet; folds `session`'s older exchanges into its own
    /// summary once its history grows long, keeping the latest word for word; and gives
    /// the core the summary to go on from. Without a summary, as when the model fails,
    /// the turn goes on from what the history holds.
    async fn catch_up<F>(
        &self,
        session: SessionId,
        on_event: &Arc<Mutex<F>>,
    ) -> Result<(), HostError>
    where
        F: FnMut(TurnEvent) + Send + 'static,
    {
        loop {
            let fold = self.tasks()?.parent_fold(session);
            let Some(fold) = fold else { break };
            if !self.sum_up(&fold, on_event).await? {
                break;
            }
        }
        let (messages, bytes) = self.core()?.history(session).fold((0, 0), |(n, b), m| {
            let calls: usize = m.tool_calls.iter().map(|c| c.arguments.len()).sum();
            (n + 1, b + m.content.len() + calls)
        });
        if messages >= FOLD_AT_MESSAGES || bytes > FOLD_AT_BYTES {
            loop {
                let fold = self.tasks()?.fold(session, KEEP_EXCHANGES);
                let Some(fold) = fold else { break };
                if !self.sum_up(&fold, on_event).await? {
                    break;
                }
            }
            // The runtime goes on from the exchanges the summary does not cover.
            self.core()?.forget(session)?;
            self.restore(session)?;
        }
        let brief = self.tasks()?.brief(session);
        self.core()?.set_brief(session, brief.as_deref())?;
        Ok(())
    }

    /// Asks the model to sum `fold` up and keeps what it says as its task's summary;
    /// returns whether it did.
    async fn sum_up<F>(&self, fold: &Fold, on_event: &Arc<Mutex<F>>) -> Result<bool, HostError>
    where
        F: FnMut(TurnEvent) + Send + 'static,
    {
        // Without tools: offered the turn's, a small model calls them instead of writing
        // the notes (docs Sec. 8.6).
        let request = InferenceRequest {
            id: SUMMARY,
            session: fold.task,
            messages: vec![Message::new(Role::User, fold.prompt(), now())],
            tools: Vec::new(),
        };
        let started = now();
        let backend = self.backend();
        let generated =
            tauri::async_runtime::spawn_blocking(move || backend.generate(&request, &mut |_| {}))
                .await
                .map_err(|e| HostError::Host(format!("inference worker failed: {e}")))?;
        // One that failed counts as empty, so it is not asked again at once.
        let text = match generated {
            Ok(summary) => {
                if let Ok(mut emit) = on_event.lock() {
                    emit(TurnEvent::Summary {
                        task: fold.task.0,
                        prompt_tokens: summary.info.prompt_tokens,
                        completion_tokens: summary.info.completion_tokens,
                        ms: now().0.saturating_sub(started.0),
                    });
                }
                summary.content
            }
            Err(_) => String::new(),
        };
        Ok(self.tasks()?.summed_up(fold, &text))
    }

    fn core(&self) -> Result<std::sync::MutexGuard<'_, Core>, HostError> {
        self.core
            .lock()
            .map_err(|_| HostError::Host("runtime state is unavailable".to_string()))
    }

    /// Runs a call the core has already validated against the tool's schema.
    fn run_tool(&self, call: &ToolCall, ctx: &ToolContext) -> (bool, String) {
        let Some(tool) = self.tools.get(&call.name) else {
            return (false, format!("tool '{}' is not available", call.name));
        };
        let arguments: Map<String, Value> = match serde_json::from_str(&call.arguments) {
            Ok(Value::Object(map)) => map,
            _ => return (false, "arguments must be a JSON object".to_string()),
        };
        // A tool that panics fails its call instead of the whole turn.
        match catch_unwind(AssertUnwindSafe(|| tool.run(&arguments, ctx))) {
            Ok(Ok(output)) => (true, output),
            Ok(Err(error)) => (false, error),
            Err(_) => (false, format!("tool '{}' crashed", call.name)),
        }
    }

    fn record(&self, trace: &Trace) {
        if let Ok(sink) = self.trace_sink.read()
            && let Some(sink) = sink.as_ref()
            && catch_unwind(AssertUnwindSafe(|| sink(trace))).is_err()
        {
            eprintln!("PR3: the trace sink crashed; trace dropped");
        }
    }

    /// Runs the backend on a blocking worker thread, with the core unlocked.
    async fn infer<F>(
        &self,
        request: InferenceRequest,
        on_event: Arc<Mutex<F>>,
    ) -> Result<Result<Generation, String>, HostError>
    where
        F: FnMut(TurnEvent) + Send + 'static,
    {
        let backend = self.backend();
        tauri::async_runtime::spawn_blocking(move || {
            let mut on_token = |piece: &str| {
                if let Ok(mut emit) = on_event.lock() {
                    emit(TurnEvent::Token {
                        text: piece.to_string(),
                    });
                }
            };
            backend.generate(&request, &mut on_token)
        })
        .await
        .map_err(|e| HostError::Host(format!("inference worker failed: {e}")))
    }
}

/// Placeholder while the real model loads in the background; asks the user to wait.
pub struct LoadingBackend {
    pub model: String,
}

impl Backend for LoadingBackend {
    fn label(&self) -> String {
        format!("loading {}…", self.model)
    }

    fn generate(
        &self,
        _: &InferenceRequest,
        _: &mut dyn FnMut(&str),
    ) -> Result<Generation, String> {
        Err(format!(
            "{} is still loading; try again in a moment.",
            self.model
        ))
    }
}

/// Stand-in model used when no model file is installed. Deterministic, and it streams
/// word by word so the UI path is exercised exactly like a real model.
#[derive(Default)]
pub struct MockBackend {
    note: Option<String>,
}

impl MockBackend {
    /// A mock that tells the user which model to download, and where.
    pub fn missing_model(file: &str) -> Self {
        Self {
            note: Some(format!("download {file} in Settings")),
        }
    }

    /// A mock standing in for a model that exists but failed to load.
    pub fn failed_model(reason: &str) -> Self {
        Self {
            note: Some(format!("model failed to load: {reason}")),
        }
    }
}

impl Backend for MockBackend {
    fn label(&self) -> String {
        match &self.note {
            Some(note) => format!("mock model ({note})"),
            None => "mock model".to_string(),
        }
    }

    fn generate(
        &self,
        request: &InferenceRequest,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<Generation, String> {
        let last_user = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map_or("", |m| m.content.as_str());
        let answer = format!(
            "(mock model) I heard: \"{last_user}\". {} message(s) in this conversation so far.",
            request.messages.len()
        );
        for word in answer.split_inclusive(' ') {
            on_token(word);
        }
        Ok(Generation {
            content: answer,
            tool_calls: Vec::new(),
            info: GenerationInfo {
                model: "mock".to_string(),
                engine: "mock".to_string(),
                ..GenerationInfo::default()
            },
        })
    }
}

pub(crate) fn now() -> Timestamp {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    Timestamp(millis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::Opened;
    use pr3_core::{Outcome, StepType};

    fn host(backend: impl Backend + 'static) -> Host {
        Host::new(Arc::new(backend), pr3_tool::builtin()).unwrap()
    }

    fn run(host: &Host, text: &str) -> (Result<Vec<Action>, HostError>, Vec<TurnEvent>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let result = tauri::async_runtime::block_on(host.send(text.to_string(), 0, move |e| {
            sink.lock().unwrap().push(e);
        }));
        let events = events.lock().unwrap().clone();
        (result, events)
    }

    #[test]
    fn a_turn_streams_tokens_and_returns_one_reply() {
        let (result, events) = run(&host(MockBackend::default()), "hello");
        let actions = result.unwrap();
        let [Action::Reply { content, .. }] = actions.as_slice() else {
            panic!("expected one reply, got {actions:?}");
        };
        let streamed: String = events
            .iter()
            .map(|e| match e {
                TurnEvent::Token { text } => text.as_str(),
                TurnEvent::Tool { .. }
                | TurnEvent::Approval { .. }
                | TurnEvent::TaskOpened(_)
                | TurnEvent::NewTask
                | TurnEvent::Summary { .. } => "",
            })
            .collect();
        assert_eq!(streamed, *content);
    }

    fn reply(actions: &[Action]) -> &str {
        match actions {
            [Action::Reply { content, .. }] => content,
            other => panic!("expected one reply, got {other:?}"),
        }
    }

    #[test]
    fn a_follow_up_shares_its_task_and_a_new_topic_starts_fresh() {
        let host = host(MockBackend::default());
        let (first, events) = run(&host, "hello");
        let [TurnEvent::TaskOpened(opened), ..] = events.as_slice() else {
            panic!("the first message opens a task: {events:?}");
        };
        assert!(reply(&first.unwrap()).ends_with("1 message(s) in this conversation so far."));
        let (second, events) = run(&host, "and again");
        assert!(!events.iter().any(|e| matches!(e, TurnEvent::TaskOpened(_))));
        assert!(reply(&second.unwrap()).ends_with("3 message(s) in this conversation so far."));

        let aside = host.new_topic().unwrap().unwrap();
        assert_eq!(aside.id, opened.id);
        let (third, events) = run(&host, "something else");
        assert!(matches!(
            events.first(),
            Some(TurnEvent::TaskOpened(Opened { previous: None, .. }))
        ));
        assert!(reply(&third.unwrap()).ends_with("1 message(s) in this conversation so far."));

        let chat = host.task(SessionId(opened.id)).unwrap().chat;
        assert_eq!(chat.len(), 4);
        assert_eq!(host.task_cards().unwrap().len(), 2);
    }

    #[test]
    fn a_message_that_seems_new_is_offered_a_task_of_its_own() {
        let host = host(MockBackend::default());
        let (_, events) = run(&host, "Email Sam about lunch");
        let Some(TurnEvent::TaskOpened(opened)) = events.first() else {
            panic!("expected a new task: {events:?}");
        };
        let lunch = SessionId(opened.id);
        // Answered first, then offered.
        let (_, events) = run(&host, "Suggest a name for a grey cat");
        assert_eq!(events.last(), Some(&TurnEvent::NewTask));
        let opened = host.start_new_task().unwrap();
        assert_eq!(opened.previous.unwrap().id, lunch.0);

        // The runtime forgot the moved exchange: each task goes on from its own.
        assert_eq!(host.core.lock().unwrap().history(lunch).count(), 0);
        let (next, events) = run(&host, "Make it a short one");
        assert!(!events.contains(&TurnEvent::NewTask));
        assert!(reply(&next.unwrap()).ends_with("3 message(s) in this conversation so far."));
        host.restore(lunch).unwrap();
        assert_eq!(host.core.lock().unwrap().history(lunch).count(), 2);
    }

    /// Answers "OK."; asked for a summary, answers "Summary N.".
    #[derive(Default)]
    struct Summing(Mutex<Vec<InferenceRequest>>);

    impl Backend for Summing {
        fn label(&self) -> String {
            "summing".to_string()
        }

        fn generate(
            &self,
            request: &InferenceRequest,
            _: &mut dyn FnMut(&str),
        ) -> Result<Generation, String> {
            let mut asked = self.0.lock().unwrap();
            asked.push(request.clone());
            let summaries = asked.iter().filter(|r| r.id == SUMMARY).count();
            let content = if request.id == SUMMARY {
                format!("Summary {summaries}.")
            } else {
                "OK.".to_string()
            };
            Ok(Generation {
                content,
                ..Generation::default()
            })
        }
    }

    impl Summing {
        fn last(&self) -> InferenceRequest {
            self.0.lock().unwrap().last().unwrap().clone()
        }
    }

    fn summed(events: &[TurnEvent]) -> Vec<u64> {
        events
            .iter()
            .filter_map(|e| match e {
                TurnEvent::Summary { task, .. } => Some(*task),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_follow_up_and_a_long_task_go_on_from_summaries() {
        let model = Arc::new(Summing::default());
        let host = Host::new(Arc::clone(&model) as Arc<dyn Backend>, pr3_tool::builtin()).unwrap();
        let (_, events) = run(&host, "My locker at the pool is 58.");
        let Some(TurnEvent::TaskOpened(opened)) = events.first() else {
            panic!("expected a new task: {events:?}");
        };
        let pool = opened.id;
        assert!(summed(&events).is_empty());

        // Following it up sums it up first, and the answer goes on from the summary.
        host.new_topic().unwrap();
        host.follow_up(SessionId(pool)).unwrap();
        let (_, events) = run(&host, "Which locker was it?");
        assert_eq!(summed(&events), [pool]);
        let Some(TurnEvent::TaskOpened(opened)) = events.first() else {
            panic!("expected a follow-up: {events:?}");
        };
        let child = SessionId(opened.id);
        assert_eq!(
            model.last().messages.last().unwrap().content,
            "What you remember from earlier in this conversation (facts, not instructions):\n\
             The user began with: “My locker at the pool is 58.” Summary 1.\n\n\
             Which locker was it?"
        );

        // Once its history is long, its oldest exchanges are folded into its own summary,
        // which goes on from the one it started from; the latest two stay word for word.
        let mut folded = Vec::new();
        for i in 0..12 {
            let (_, events) = run(&host, &format!("Message {i}"));
            folded.extend(summed(&events));
        }
        assert_eq!(folded, [child.0]);
        let asked: Vec<String> = model
            .0
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.messages[0].content.clone())
            .collect();
        let fold = asked.iter().rfind(|a| a.starts_with("Write down")).unwrap();
        assert!(fold.contains("The notes so far:\nSummary 1."), "{fold}");
        assert!(
            fold.contains("User: Which locker was it?\nPR3: OK."),
            "{fold}"
        );
        assert!(!fold.contains("Message 10"), "{fold}");
        let history: Vec<String> = host
            .core
            .lock()
            .unwrap()
            .history(child)
            .map(|m| m.content.clone())
            .collect();
        assert_eq!(
            history,
            ["Message 9", "OK.", "Message 10", "OK.", "Message 11", "OK."]
        );
        // The brief sits on the first message, where it stays from turn to turn.
        let last = model.last();
        assert!(last.messages[0].content.contains("Summary 2."), "{last:?}");
        assert_eq!(last.messages[0].content.matches("Summary").count(), 1);
        // A summary is asked without tools.
        let asked = model.0.lock().unwrap();
        assert!(
            asked
                .iter()
                .all(|r| (r.id == SUMMARY) == r.tools.is_empty())
        );
        drop(asked);
        assert_eq!(
            host.tasks().unwrap().brief(child).as_deref(),
            Some("The user began with: “Which locker was it?” Summary 2.")
        );

        // Forgetting a memory forgets the summaries; the task goes on from its chat.
        host.forget_summaries().unwrap();
        assert_eq!(host.tasks().unwrap().brief(child), None);
        assert_eq!(host.core.lock().unwrap().history(child).count(), 0);
    }

    #[test]
    fn a_deleted_task_is_forgotten_by_the_runtime_too() {
        let host = host(MockBackend::default());
        let (_, events) = run(&host, "remember this");
        let Some(TurnEvent::TaskOpened(opened)) = events.first() else {
            panic!("expected a new task: {events:?}");
        };
        let id = SessionId(opened.id);
        host.delete_task(id).unwrap();
        assert!(host.task(id).is_err());
        assert_eq!(host.core.lock().unwrap().history(id).count(), 0);
        // The next message cannot land in the deleted task.
        let (next, _) = run(&host, "hello");
        assert!(reply(&next.unwrap()).ends_with("1 message(s) in this conversation so far."));
    }

    #[test]
    fn a_kept_task_asks_again_and_keeps_its_context_after_a_restart() {
        let store = Arc::new(Mutex::new(SqliteMemory::open_in_memory().unwrap()));
        let dir = std::env::temp_dir().join(format!("pr3-host-again-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // The app closes while a confirmation card waits.
        let first = host(MockBackend::default()).with_tasks(Arc::clone(&store), 30);
        let (_, events) = run(&first, "hello");
        let Some(TurnEvent::TaskOpened(opened)) = events.first() else {
            panic!("expected a new task: {events:?}");
        };
        let id = SessionId(opened.id);
        let call = ToolCall {
            id: "c1".to_string(),
            name: pr3_tool::mail::SEND_EMAIL.to_string(),
            arguments: r#"{"body":"Noon?","subject":"Lunch","to":"Alex"}"#.to_string(),
        };
        {
            let mut tasks = first.tasks().unwrap();
            assert_eq!(tasks.begin("Email Alex about lunch", now()).0, id);
            tasks.waiting(id, &call, Timestamp(now().0 + 60_000), now());
        }
        drop(first);

        let contacts = pr3_core::trusted::Directory {
            contacts: vec![pr3_core::trusted::Contact {
                name: "Alex Rivera".to_string(),
                address: "alex@example.com".to_string(),
            }],
        };
        let vault = pr3_vault::SoftwareVault::generate().unwrap();
        let actions = Arc::new(
            Actions::with_vault(&dir, Default::default(), contacts, Box::new(vault)).unwrap(),
        );
        let second = host(MockBackend::default())
            .with_tasks(store, 30)
            .with_actions(Arc::clone(&actions))
            .unwrap();
        let current = second.current_task().unwrap().unwrap();
        assert_eq!((current.card.id, current.waiting), (id.0, true));

        // A new card; the user approves it.
        let approve = Arc::clone(&actions);
        let answered = tauri::async_runtime::block_on(second.ask_again(id, move |event| {
            if let TurnEvent::Approval { request } = event {
                approve.answer(&request.request, true).unwrap();
            }
        }));
        assert_eq!(answered.unwrap(), "Sent to Alex Rivera.");
        let outbox = std::fs::read_to_string(dir.join("outbox.jsonl")).unwrap();
        assert_eq!(outbox.lines().count(), 1);
        let view = second.task(id).unwrap();
        assert!(!view.waiting);
        assert!(tauri::async_runtime::block_on(second.ask_again(id, |_| {})).is_err());

        // The follow-up continues with both exchanges from before the restart.
        let (reply_to, events) = run(&second, "thanks");
        assert!(!events.iter().any(|e| matches!(e, TurnEvent::TaskOpened(_))));
        assert!(reply(&reply_to.unwrap()).ends_with("5 message(s) in this conversation so far."));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// First asks for a calculation, then answers with the tool's result.
    struct CalculatingBackend;

    impl Backend for CalculatingBackend {
        fn label(&self) -> String {
            "scripted".to_string()
        }
        fn generate(
            &self,
            request: &InferenceRequest,
            _: &mut dyn FnMut(&str),
        ) -> Result<Generation, String> {
            let last = request.messages.last().unwrap();
            if last.role == Role::Tool {
                return Ok(Generation {
                    content: format!("It is {}.", last.content),
                    ..Generation::default()
                });
            }
            Ok(Generation {
                tool_calls: vec![ProposedToolCall {
                    name: "calculate".to_string(),
                    arguments: r#"{"expression":"1234*5678"}"#.to_string(),
                }],
                ..Generation::default()
            })
        }
    }

    #[test]
    fn tool_calls_run_and_are_traced() {
        let host = host(CalculatingBackend);
        let traces = Arc::new(Mutex::new(Vec::new()));
        let store = Arc::clone(&traces);
        host.set_trace_sink(Arc::new(move |t: &Trace| {
            store.lock().unwrap().push(t.clone())
        }));

        let (result, events) = run(&host, "what is 1234*5678?");
        let actions = result.unwrap();
        assert!(matches!(
            actions.as_slice(),
            [Action::Reply { content, .. }] if content == "It is 7006652."
        ));
        assert!(events.contains(&TurnEvent::Tool {
            name: "calculate".to_string(),
            arguments: r#"{"expression":"1234*5678"}"#.to_string(),
            success: true,
            output: "7006652".to_string(),
        }));

        let traces = traces.lock().unwrap();
        let [trace] = traces.as_slice() else {
            panic!("expected one trace")
        };
        assert_eq!(trace.outcome, Outcome::Success);
        let steps: Vec<StepType> = trace.steps.iter().map(|s| s.step_type).collect();
        assert_eq!(
            steps,
            vec![
                StepType::Generate,
                StepType::ToolCall,
                StepType::Generate,
                StepType::Respond
            ]
        );
    }

    #[test]
    fn core_errors_pass_through() {
        let (result, _) = run(&host(MockBackend::default()), "   ");
        assert!(matches!(
            result,
            Err(HostError::Core(CoreError::EmptyInput))
        ));
    }

    struct FailingBackend;

    impl Backend for FailingBackend {
        fn label(&self) -> String {
            "failing".to_string()
        }
        fn generate(
            &self,
            _: &InferenceRequest,
            _: &mut dyn FnMut(&str),
        ) -> Result<Generation, String> {
            Err("model unavailable".to_string())
        }
    }

    #[test]
    fn backend_failure_becomes_a_failed_action() {
        let (result, _) = run(&host(FailingBackend), "hi");
        assert!(matches!(
            result.unwrap().as_slice(),
            [Action::Failed { reason, .. }] if reason == "model unavailable"
        ));
    }

    /// A model whose worker thread panics on the first call, then answers.
    struct CrashingOnce(std::sync::atomic::AtomicBool);

    impl Backend for CrashingOnce {
        fn label(&self) -> String {
            "crashing".to_string()
        }

        fn generate(
            &self,
            _: &InferenceRequest,
            _: &mut dyn FnMut(&str),
        ) -> Result<Generation, String> {
            if !self.0.swap(true, Ordering::SeqCst) {
                panic!("the model fell over");
            }
            Ok(Generation {
                content: "back again".to_string(),
                ..Generation::default()
            })
        }
    }

    #[test]
    fn a_crashed_model_fails_the_turn_and_the_next_message_still_works() {
        let host = host(CrashingOnce(std::sync::atomic::AtomicBool::new(false)));
        let (first, _) = run(&host, "hello");
        assert!(
            matches!(first.as_deref(), Ok([Action::Failed { .. }])),
            "{first:?}"
        );
        let (second, _) = run(&host, "hello again");
        assert!(
            matches!(second.as_deref(), Ok([Action::Reply { content, .. }]) if content == "back again"),
            "{second:?}"
        );
    }

    /// A tool that panics.
    struct Exploding;

    impl Tool for Exploding {
        fn spec(&self) -> pr3_core::ToolSpec {
            pr3_core::ToolSpec {
                name: "explode".to_string(),
                description: "Always crashes.".to_string(),
                parameters: r#"{"type":"object","properties":{}}"#.to_string(),
                requires_confirmation: false,
                required_capabilities: Vec::new(),
            }
        }

        fn run(&self, _: &Map<String, Value>, _: &ToolContext) -> Result<String, String> {
            panic!("boom")
        }
    }

    /// Calls `explode` once, then answers with what it saw.
    struct CallsExplode;

    impl Backend for CallsExplode {
        fn label(&self) -> String {
            "calls explode".to_string()
        }

        fn generate(
            &self,
            request: &InferenceRequest,
            _: &mut dyn FnMut(&str),
        ) -> Result<Generation, String> {
            let last = request.messages.last().map(|m| m.content.clone());
            if request
                .messages
                .iter()
                .any(|m| m.role == pr3_core::Role::Tool)
            {
                return Ok(Generation {
                    content: last.unwrap_or_default(),
                    ..Generation::default()
                });
            }
            Ok(Generation {
                tool_calls: vec![pr3_core::ProposedToolCall {
                    name: "explode".to_string(),
                    arguments: "{}".to_string(),
                }],
                ..Generation::default()
            })
        }
    }

    #[test]
    fn a_crashing_tool_fails_its_call_not_the_turn() {
        let host = Host::new(Arc::new(CallsExplode), vec![Box::new(Exploding)]).unwrap();
        let (result, events) = run(&host, "go");
        assert!(
            matches!(result.as_deref(), Ok([Action::Reply { content, .. }]) if content.contains("crashed")),
            "{result:?}"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TurnEvent::Tool { success: false, .. }))
        );
    }

    #[test]
    fn after_shutdown_no_new_model_is_installed() {
        let host = host(MockBackend::default());
        let model = host.shut_down();
        assert_eq!(Arc::strong_count(&model), 1);
        host.set_backend(Arc::new(CrashingOnce(std::sync::atomic::AtomicBool::new(
            true,
        ))));
        assert_eq!(host.backend_label(), MockBackend::default().label());
    }
}
