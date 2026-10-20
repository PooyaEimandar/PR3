use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::types::{Message, RequestId, SessionId, Timestamp};

/// Everything the host can tell the core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Input {
    /// The user said (voice, already transcribed) or typed something.
    Utterance {
        session: SessionId,
        content: String,
        at: Timestamp,
    },
    /// The host finished an [`Action::Infer`].
    ModelOutput {
        request: RequestId,
        content: String,
        at: Timestamp,
    },
    /// The host could not run an [`Action::Infer`].
    ModelFailed {
        request: RequestId,
        reason: String,
        at: Timestamp,
    },
}

/// Everything the core asks the host to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Run a model on this conversation and answer with `Input::ModelOutput`.
    Infer(InferenceRequest),
    /// Show or speak a reply.
    Reply { session: SessionId, content: String },
    /// Tell the user a turn failed; nothing was done on their behalf.
    Failed { session: SessionId, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub id: RequestId,
    pub session: SessionId,
    pub messages: Vec<Message>,
}
