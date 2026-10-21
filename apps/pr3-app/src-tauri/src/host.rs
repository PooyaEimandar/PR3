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
    Action, Config, Core, CoreError, GenerationInfo, InferenceRequest, Input, ProposedToolCall,
    Role, SessionId, Timestamp, ToolCall, Trace,
};
use pr3_tools::{SharedMemory, Tool, ToolContext};

use crate::actions::Actions;
use serde::Serialize;
use serde_json::{Map, Value};

/// Upper bound on host/core round trips per user turn; a backstop, not the normal exit.
/// The core's own limits (tool rounds, loop guard) end a turn long before this.
const MAX_ROUND_TRIPS: usize = 64;

/// Longer messages are not looked up in memory: a long paste is not a question about
/// the user, and the lookup runs before the core checks the message's size.
const MAX_LOOKUP_BYTES: usize = 4 * 1024;

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
    session: SessionId,
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
            // The launch time keeps trace ids (`pr3-{session}-{turn}`) unique across
            // launches, which OpenJarvis's trace store needs.
            session: SessionId(now().0),
        })
    }

    /// Looks up each message in `memory` and gives the matches to the model as notes.
    pub fn with_memory(mut self, memory: SharedMemory) -> Self {
        self.memory = Some(memory);
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

    pub(crate) fn backend(&self) -> Arc<dyn Backend> {
        match self.backend.read() {
            Ok(b) => Arc::clone(&b),
            Err(poisoned) => Arc::clone(&poisoned.into_inner()),
        }
    }

    /// Runs one user turn and returns what the UI should show at the end (a reply or a
    /// failure). Tokens and tool activity stream through `on_event` meanwhile.
    pub async fn send<F>(
        &self,
        text: String,
        utc_offset_minutes: i32,
        on_event: F,
    ) -> Result<Vec<Action>, HostError>
    where
        F: FnMut(TurnEvent) + Send + 'static,
    {
        let utterance = text.clone();
        let notes = match &self.memory {
            Some(memory) if text.len() <= MAX_LOOKUP_BYTES => {
                pr3_tools::notes_for(memory, &text, now())
            }
            _ => Vec::new(),
        };
        // If the core refuses the message, no turn was started.
        let queue = self.step(Input::Utterance {
            session: self.session,
            content: text,
            notes,
            at: now(),
        })?;
        let on_event = Arc::new(Mutex::new(on_event));
        match self
            .drive(queue, utc_offset_minutes, &utterance, on_event)
            .await
        {
            Ok(for_ui) => Ok(for_ui),
            // The turn is still open: end it, so the session takes the next message.
            Err(error) => {
                let actions = self.step(Input::Abort {
                    session: self.session,
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
                        session: self.session,
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
        let mut core = self
            .core
            .lock()
            .map_err(|_| HostError::Host("runtime state is unavailable".to_string()))?;
        Ok(core.step(input)?)
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
    /// A mock that tells the user which model file it is standing in for, and where.
    pub fn missing_model(file: &str, expected_dir: Option<String>) -> Self {
        let note = match expected_dir {
            Some(dir) => format!("no {file} in {dir}"),
            None => format!("no {file}"),
        };
        Self { note: Some(note) }
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
    use pr3_core::{Outcome, StepType};

    fn host(backend: impl Backend + 'static) -> Host {
        Host::new(Arc::new(backend), pr3_tools::builtin()).unwrap()
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
                TurnEvent::Tool { .. } | TurnEvent::Approval { .. } => "",
            })
            .collect();
        assert_eq!(streamed, *content);
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
