use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::trace::Trace;
use crate::types::{
    GenerationInfo, Message, ProposedToolCall, RequestId, SessionId, Timestamp, ToolCall, ToolSpec,
};

/// Everything the host can tell the core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Input {
    /// The user said (voice, already transcribed) or typed something.
    Utterance {
        session: SessionId,
        content: String,
        /// What the host looked up for this turn, such as matching memories. The model
        /// sees them ahead of the user's words as information, never as instructions;
        /// the trace records the user's words alone.
        notes: Vec<String>,
        at: Timestamp,
    },
    /// The host gives up on the session's turn: it lost the model, a tool crashed, or
    /// the app is shutting down. The turn ends as a failure with `reason`, so the
    /// session can take the next message.
    Abort {
        session: SessionId,
        reason: String,
        at: Timestamp,
    },
    /// The host finished an [`Action::Infer`]. `tool_calls` are the model's proposals,
    /// already parsed from its output format but not yet trusted.
    ModelOutput {
        request: RequestId,
        content: String,
        tool_calls: Vec<ProposedToolCall>,
        info: GenerationInfo,
        at: Timestamp,
    },
    /// The host could not run an [`Action::Infer`].
    ModelFailed {
        request: RequestId,
        reason: String,
        at: Timestamp,
    },
    /// The host ran an [`Action::CallTool`].
    ToolOutcome {
        call_id: String,
        success: bool,
        output: String,
        at: Timestamp,
    },
}

/// Everything the core asks the host to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Run a model on this conversation and answer with `Input::ModelOutput`.
    Infer(InferenceRequest),
    /// Run a validated tool call and answer with `Input::ToolOutcome`.
    CallTool(ToolCall),
    /// A validated call to a consequential tool (`requires_confirmation`). The host must
    /// not run it directly: it goes through the trusted side — validation, policy, the
    /// user's approval, a one-use capability and the audit log (Roadmap §4.6, see
    /// [`crate::trusted`]) — and the host answers with `Input::ToolOutcome`, which says
    /// "declined" or "denied" as often as "done".
    Propose(ToolCall),
    /// Show or speak a reply.
    Reply { session: SessionId, content: String },
    /// Tell the user a turn failed; nothing more was done on their behalf.
    Failed { session: SessionId, reason: String },
    /// The finished turn's trace, for the host to store or export.
    Trace(Trace),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub id: RequestId,
    pub session: SessionId,
    pub messages: Vec<Message>,
    /// Tools the model may call this turn.
    pub tools: Vec<ToolSpec>,
}
