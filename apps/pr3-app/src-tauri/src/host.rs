//! The host side of the sans-I/O loop: it performs the actions `pr3-core` asks for.
//!
//! The core's lock is held only for each `step()`; inference runs on a blocking worker
//! thread with the lock released, so the UI thread never waits on a model.

use std::fmt;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use pr3_core::{
    Action, Config, Core, CoreError, InferenceRequest, Input, Role, SessionId, Timestamp,
};

/// Upper bound on host/core round trips per user turn; a backstop, not the normal exit.
const MAX_ROUND_TRIPS: usize = 16;

/// A model the host can run. Host-side and `std`; the core never sees it.
pub trait Backend: Send + Sync {
    /// Short label for the UI, e.g. "mock model" or "Qwen3-0.6B Q8_0".
    fn label(&self) -> String;

    /// Answers `request`, calling `on_token` with each piece of visible text as it is
    /// produced. Returns the complete final answer.
    fn generate(
        &self,
        request: &InferenceRequest,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<String, String>;
}

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

pub struct Host {
    core: Mutex<Core>,
    /// Swappable: the app starts with a placeholder and installs the real model once
    /// it has loaded in the background.
    backend: RwLock<Arc<dyn Backend>>,
    session: SessionId,
}

impl Host {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self {
            core: Mutex::new(Core::new(Config::default())),
            backend: RwLock::new(backend),
            session: SessionId(1),
        }
    }

    pub fn backend_label(&self) -> String {
        self.backend().label()
    }

    pub fn set_backend(&self, backend: Arc<dyn Backend>) {
        if let Ok(mut current) = self.backend.write() {
            *current = backend;
        }
    }

    fn backend(&self) -> Arc<dyn Backend> {
        match self.backend.read() {
            Ok(b) => Arc::clone(&b),
            Err(poisoned) => Arc::clone(&poisoned.into_inner()),
        }
    }

    /// Runs one user turn and returns the actions the UI should show. Tokens are
    /// streamed through `on_token` while the model runs.
    pub async fn send<F>(&self, text: String, on_token: F) -> Result<Vec<Action>, HostError>
    where
        F: FnMut(&str) + Send + 'static,
    {
        let mut queue = self.step(Input::Utterance {
            session: self.session,
            content: text,
            at: now(),
        })?;
        let mut for_ui = Vec::new();
        let on_token = Arc::new(Mutex::new(on_token));

        for _ in 0..MAX_ROUND_TRIPS {
            if queue.is_empty() {
                break;
            }
            let action = queue.remove(0);
            match action {
                Action::Infer(request) => {
                    let id = request.id;
                    let outcome = self.infer(request, Arc::clone(&on_token)).await?;
                    let input = match outcome {
                        Ok(content) => Input::ModelOutput {
                            request: id,
                            content,
                            at: now(),
                        },
                        Err(reason) => Input::ModelFailed {
                            request: id,
                            reason,
                            at: now(),
                        },
                    };
                    queue.extend(self.step(input)?);
                }
                other => for_ui.push(other),
            }
        }
        if !queue.is_empty() {
            return Err(HostError::TooManyRoundTrips);
        }
        Ok(for_ui)
    }

    fn step(&self, input: Input) -> Result<Vec<Action>, HostError> {
        let mut core = self
            .core
            .lock()
            .map_err(|_| HostError::Host("runtime state is unavailable".to_string()))?;
        Ok(core.step(input)?)
    }

    /// Runs the backend on a blocking worker thread, with the core unlocked.
    async fn infer<F>(
        &self,
        request: InferenceRequest,
        on_token: Arc<Mutex<F>>,
    ) -> Result<Result<String, String>, HostError>
    where
        F: FnMut(&str) + Send + 'static,
    {
        let backend = self.backend();
        tauri::async_runtime::spawn_blocking(move || {
            let mut emit = |piece: &str| {
                if let Ok(mut f) = on_token.lock() {
                    f(piece);
                }
            };
            backend.generate(&request, &mut emit)
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

    fn generate(&self, _: &InferenceRequest, _: &mut dyn FnMut(&str)) -> Result<String, String> {
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
    ) -> Result<String, String> {
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
        Ok(answer)
    }
}

fn now() -> Timestamp {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    Timestamp(millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host() -> Host {
        Host::new(Arc::new(MockBackend::default()))
    }

    #[test]
    fn a_turn_streams_tokens_and_returns_one_reply() {
        let streamed = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&streamed);
        let actions = tauri::async_runtime::block_on(host().send("hello".to_string(), move |t| {
            sink.lock().unwrap().push_str(t);
        }))
        .unwrap();

        let [Action::Reply { content, .. }] = actions.as_slice() else {
            panic!("expected one reply, got {actions:?}");
        };
        assert!(content.contains("hello"));
        assert_eq!(*streamed.lock().unwrap(), *content);
    }

    #[test]
    fn core_errors_pass_through() {
        let result = tauri::async_runtime::block_on(host().send("   ".to_string(), |_| {}));
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
        ) -> Result<String, String> {
            Err("model unavailable".to_string())
        }
    }

    #[test]
    fn backend_failure_becomes_a_failed_action() {
        let host = Host::new(Arc::new(FailingBackend));
        let actions = tauri::async_runtime::block_on(host.send("hi".to_string(), |_| {})).unwrap();
        assert!(matches!(
            actions.as_slice(),
            [Action::Failed { reason, .. }] if reason == "model unavailable"
        ));
    }
}
