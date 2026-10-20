use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::error::CoreError;
use crate::event::{Action, InferenceRequest, Input};
use crate::types::{Message, RequestId, Role, SessionId, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// Longest accepted user message, in bytes.
    pub max_input_bytes: usize,
    /// Messages kept per session; the oldest fall off first.
    pub max_history: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_input_bytes: 16 * 1024,
            max_history: 32,
        }
    }
}

#[derive(Debug, Default)]
struct Session {
    history: VecDeque<Message>,
}

/// The PR3 runtime state. Deterministic: the same inputs always give the same actions.
#[derive(Debug)]
pub struct Core {
    config: Config,
    // BTreeMap, not HashMap: no hidden entropy dependency, HashDoS-immune (Roadmap §4.2).
    sessions: BTreeMap<SessionId, Session>,
    pending: BTreeMap<RequestId, SessionId>,
    next_request: u64,
}

impl Core {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            sessions: BTreeMap::new(),
            pending: BTreeMap::new(),
            next_request: 1,
        }
    }

    /// Advance the state machine by one input.
    pub fn step(&mut self, input: Input) -> Result<Vec<Action>, CoreError> {
        match input {
            Input::Utterance {
                session,
                content,
                at,
            } => self.on_utterance(session, content, at),
            Input::ModelOutput {
                request,
                content,
                at,
            } => {
                let session = self.take_pending(request)?;
                self.push_message(session, Role::Assistant, content.clone(), at)?;
                Ok(vec![Action::Reply { session, content }])
            }
            Input::ModelFailed {
                request, reason, ..
            } => {
                let session = self.take_pending(request)?;
                Ok(vec![Action::Failed { session, reason }])
            }
        }
    }

    /// Messages remembered for a session, oldest first.
    pub fn history(&self, session: SessionId) -> impl Iterator<Item = &Message> {
        self.sessions
            .get(&session)
            .into_iter()
            .flat_map(|s| s.history.iter())
    }

    fn on_utterance(
        &mut self,
        session: SessionId,
        content: String,
        at: Timestamp,
    ) -> Result<Vec<Action>, CoreError> {
        if content.len() > self.config.max_input_bytes {
            return Err(CoreError::InputTooLarge {
                len: content.len(),
                max: self.config.max_input_bytes,
            });
        }
        if content.trim().is_empty() {
            return Err(CoreError::EmptyInput);
        }
        self.push_message(session, Role::User, content, at)?;

        let id = RequestId(self.next_request);
        self.next_request = self.next_request.saturating_add(1);
        self.pending.insert(id, session);

        let messages = self.history(session).cloned().collect();
        Ok(vec![Action::Infer(InferenceRequest {
            id,
            session,
            messages,
        })])
    }

    fn take_pending(&mut self, request: RequestId) -> Result<SessionId, CoreError> {
        self.pending
            .remove(&request)
            .ok_or(CoreError::UnknownRequest(request))
    }

    fn push_message(
        &mut self,
        session: SessionId,
        role: Role,
        content: String,
        at: Timestamp,
    ) -> Result<(), CoreError> {
        let max = self.config.max_history.max(1);
        let history = &mut self.sessions.entry(session).or_default().history;
        while history.len() >= max {
            history.pop_front();
        }
        // Fallible allocation at an untrusted boundary (Roadmap §4.2).
        history.try_reserve(1).map_err(|_| CoreError::OutOfMemory)?;
        history.push_back(Message { role, content, at });
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    const S: SessionId = SessionId(1);

    fn user(text: &str) -> Input {
        Input::Utterance {
            session: S,
            content: text.to_string(),
            at: Timestamp(10),
        }
    }

    fn infer_id(actions: &[Action]) -> RequestId {
        match actions {
            [Action::Infer(req)] => req.id,
            other => unreachable!("expected one Infer action, got {other:?}"),
        }
    }

    #[test]
    fn user_message_asks_host_to_infer_then_replies() {
        let mut core = Core::new(Config::default());
        let actions = core.step(user("hello")).unwrap();
        let id = infer_id(&actions);

        let actions = core
            .step(Input::ModelOutput {
                request: id,
                content: "hi".to_string(),
                at: Timestamp(11),
            })
            .unwrap();
        assert_eq!(
            actions,
            vec![Action::Reply {
                session: S,
                content: "hi".to_string()
            }]
        );
        let roles: Vec<Role> = core.history(S).map(|m| m.role).collect();
        assert_eq!(roles, vec![Role::User, Role::Assistant]);
    }

    #[test]
    fn answering_unknown_or_repeated_request_is_rejected() {
        let mut core = Core::new(Config::default());
        let id = infer_id(&core.step(user("hello")).unwrap());
        let answer = Input::ModelOutput {
            request: id,
            content: "hi".to_string(),
            at: Timestamp(11),
        };
        assert!(core.step(answer.clone()).is_ok());
        assert_eq!(core.step(answer), Err(CoreError::UnknownRequest(id)));
    }

    #[test]
    fn oversized_and_empty_input_are_rejected() {
        let mut core = Core::new(Config {
            max_input_bytes: 4,
            max_history: 8,
        });
        assert_eq!(
            core.step(user("too long")),
            Err(CoreError::InputTooLarge { len: 8, max: 4 })
        );
        assert_eq!(core.step(user("  ")), Err(CoreError::EmptyInput));
    }

    #[test]
    fn history_is_bounded() {
        let mut core = Core::new(Config {
            max_input_bytes: 64,
            max_history: 3,
        });
        for i in 0..5 {
            let _ = core.step(user(&i.to_string())).unwrap();
        }
        let texts: Vec<&str> = core.history(S).map(|m| m.content.as_str()).collect();
        assert_eq!(texts, vec!["2", "3", "4"]);
    }

    #[test]
    fn failed_inference_reports_failure() {
        let mut core = Core::new(Config::default());
        let id = infer_id(&core.step(user("hello")).unwrap());
        let actions = core
            .step(Input::ModelFailed {
                request: id,
                reason: "model unavailable".to_string(),
                at: Timestamp(12),
            })
            .unwrap();
        assert_eq!(
            actions,
            vec![Action::Failed {
                session: S,
                reason: "model unavailable".to_string()
            }]
        );
    }
}
