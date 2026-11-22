//! The step interface, borrowed both ways. An [`Input`] borrows the host's buffers for
//! one call; the [`Action`]s that come back borrow the core and stay valid until the next
//! call that changes it. Nothing is copied to cross the boundary. The crate root has the
//! owned forms, for hosts that queue, store or send them.

use alloc::vec::Vec;

use crate::owned;
use crate::trace::TraceView;
use crate::types::{
    GenerationInfo, Message, ProposedToolCall, RequestId, Role, SessionId, Timestamp, ToolCall,
    ToolSpec,
};

/// Everything the host can tell the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input<'a> {
    /// The user said (voice, already transcribed) or typed something.
    Utterance {
        session: SessionId,
        content: &'a str,
        /// What the host looked up for this turn, such as matching memories. The model
        /// sees them ahead of the user's words as information, never as instructions;
        /// the trace records the user's words alone.
        notes: &'a [&'a str],
        at: Timestamp,
    },
    /// The host gives up on the session's turn. The turn ends as a failure with `reason`.
    Abort {
        session: SessionId,
        reason: &'a str,
        at: Timestamp,
    },
    /// The host finished an [`Action::Infer`]. `tool_calls` are the model's proposals,
    /// already parsed from its output format but not yet trusted.
    ModelOutput {
        request: RequestId,
        content: &'a str,
        tool_calls: &'a [ProposedToolCall<'a>],
        info: GenerationInfo<'a>,
        at: Timestamp,
    },
    /// The host could not run an [`Action::Infer`].
    ModelFailed {
        request: RequestId,
        reason: &'a str,
        at: Timestamp,
    },
    /// The host ran an [`Action::CallTool`].
    ToolOutcome {
        call_id: &'a str,
        success: bool,
        output: &'a str,
        /// Origins reported by the host, separate from the untrusted result text.
        sources: &'a [&'a str],
        at: Timestamp,
    },
}

/// Everything the core asks the host to do, borrowed from the core.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action<'c> {
    /// Run a model on this conversation and answer with `Input::ModelOutput`.
    Infer(InferenceRequest<'c>),
    /// Run a validated tool call and answer with `Input::ToolOutcome`.
    CallTool(&'c ToolCall),
    /// A validated call to a consequential tool (`requires_confirmation`), for the
    /// trusted side (see [`crate::trusted`]); answered with `Input::ToolOutcome`.
    Propose(&'c ToolCall),
    /// Show or speak a reply.
    Reply {
        session: SessionId,
        content: &'c str,
    },
    /// Tell the user a turn failed; nothing more was done on their behalf.
    Failed { session: SessionId, reason: &'c str },
    /// The finished turn's trace, for the host to store or export.
    Trace(TraceView<'c>),
}

impl Action<'_> {
    pub fn to_owned(&self) -> owned::Action {
        match *self {
            Self::Infer(r) => owned::Action::Infer(r.to_owned()),
            Self::CallTool(c) => owned::Action::CallTool(c.clone()),
            Self::Propose(c) => owned::Action::Propose(c.clone()),
            Self::Reply { session, content } => owned::Action::Reply {
                session,
                content: content.into(),
            },
            Self::Failed { session, reason } => owned::Action::Failed {
                session,
                reason: reason.into(),
            },
            Self::Trace(t) => owned::Action::Trace(t.to_owned()),
        }
    }
}

/// One message of a request as the model sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageView<'c> {
    pub role: Role,
    pub content: &'c str,
    pub at: Timestamp,
    pub tool_calls: &'c [ToolCall],
    pub tool_call_id: Option<&'c str>,
}

impl MessageView<'_> {
    pub fn to_owned(&self) -> Message {
        Message {
            role: self.role,
            content: self.content.into(),
            at: self.at,
            tool_calls: self.tool_calls.to_vec(),
            tool_call_id: self.tool_call_id.map(Into::into),
        }
    }
}

/// The conversation the model reads, lent from the session: its history, the brief on
/// the first user message, and the round budget as a last system message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InferenceRequest<'c> {
    pub id: RequestId,
    pub session: SessionId,
    /// The session's history ring, as its two slices.
    pub(crate) history: (&'c [Message], &'c [Message]),
    /// Which message carries the brief, and that message as the model sees it.
    pub(crate) brief: Option<(usize, &'c str)>,
    pub(crate) budget: Option<(&'c str, Timestamp)>,
    /// Tools the model may call this turn.
    pub tools: &'c [ToolSpec],
}

impl<'c> InferenceRequest<'c> {
    pub fn len(&self) -> usize {
        self.history.0.len() + self.history.1.len() + usize::from(self.budget.is_some())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn message(&self, index: usize) -> Option<MessageView<'c>> {
        let (first, second) = self.history;
        let m = first
            .get(index)
            .or_else(|| second.get(index.checked_sub(first.len())?));
        if let Some(m) = m {
            let content = match self.brief {
                Some((at, joined)) if at == index => joined,
                _ => m.content.as_str(),
            };
            return Some(MessageView {
                role: m.role,
                content,
                at: m.at,
                tool_calls: &m.tool_calls,
                tool_call_id: m.tool_call_id.as_deref(),
            });
        }
        let (content, at) = self
            .budget
            .filter(|_| index == first.len() + second.len())?;
        Some(MessageView {
            role: Role::System,
            content,
            at,
            tool_calls: &[],
            tool_call_id: None,
        })
    }

    pub fn messages(&self) -> impl Iterator<Item = MessageView<'c>> + use<'c> {
        let request = *self;
        (0..request.len()).filter_map(move |i| request.message(i))
    }

    pub fn to_owned(&self) -> owned::InferenceRequest {
        owned::InferenceRequest {
            id: self.id,
            session: self.session,
            messages: self.messages().map(|m| m.to_owned()).collect::<Vec<_>>(),
            tools: self.tools.to_vec(),
        }
    }
}
