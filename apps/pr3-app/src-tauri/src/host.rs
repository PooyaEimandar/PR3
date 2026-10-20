//! The host side of the sans-I/O loop: it performs the actions `pr3-core` asks for.

use std::collections::VecDeque;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use pr3_core::{
    Action, Config, Core, CoreError, InferenceRequest, Input, Role, SessionId, Timestamp,
};

/// Upper bound on host/core round trips per user turn; a backstop, not the normal exit.
const MAX_ROUND_TRIPS: usize = 16;

#[derive(Debug)]
pub enum HostError {
    Core(CoreError),
    /// The core still had work queued after `MAX_ROUND_TRIPS`; the turn is abandoned
    /// rather than silently dropping what was left.
    TooManyRoundTrips,
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
        }
    }
}

pub struct Host {
    core: Core,
    session: SessionId,
}

impl Host {
    pub fn new() -> Self {
        Self {
            core: Core::new(Config::default()),
            session: SessionId(1),
        }
    }

    /// Feeds one user message and drives the core until only UI actions remain.
    pub fn send(&mut self, text: String) -> Result<Vec<Action>, HostError> {
        let first = self.core.step(Input::Utterance {
            session: self.session,
            content: text,
            at: now(),
        })?;
        let mut queue = VecDeque::from(first);
        let mut for_ui = Vec::new();

        for _ in 0..MAX_ROUND_TRIPS {
            let Some(action) = queue.pop_front() else {
                break;
            };
            match action {
                Action::Infer(request) => {
                    let content = mock_infer(&request);
                    queue.extend(self.core.step(Input::ModelOutput {
                        request: request.id,
                        content,
                        at: now(),
                    })?);
                }
                other => for_ui.push(other),
            }
        }
        if !queue.is_empty() {
            return Err(HostError::TooManyRoundTrips);
        }
        Ok(for_ui)
    }
}

/// Stand-in model until step 2 wires Qwen through `pr3-llamacpp`. Deterministic, so
/// the full loop can be exercised on every platform before any model is bundled.
fn mock_infer(request: &InferenceRequest) -> String {
    let last_user = request
        .messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .map_or("", |m| m.content.as_str());
    format!(
        "(mock model) I heard: \"{last_user}\". {} message(s) in this conversation so far.",
        request.messages.len()
    )
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

    #[test]
    fn a_turn_returns_exactly_one_reply_for_the_ui() {
        let mut host = Host::new();
        let actions = host.send("hello".to_string()).unwrap();
        assert!(
            matches!(actions.as_slice(), [Action::Reply { content, .. }] if content.contains("hello"))
        );
    }

    #[test]
    fn core_errors_pass_through() {
        let mut host = Host::new();
        assert!(matches!(
            host.send("   ".to_string()),
            Err(HostError::Core(CoreError::EmptyInput))
        ));
    }
}
