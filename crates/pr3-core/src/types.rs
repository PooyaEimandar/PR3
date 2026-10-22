use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// Milliseconds since the Unix epoch, always supplied by the host clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Timestamp(pub u64);

/// A conversation, chosen by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SessionId(pub u64);

/// Correlates an [`crate::Action::Infer`] with the host's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RequestId(pub u64);

/// Names follow OpenJarvis's core types (`Role`, `Message{role, content}`,
/// `ToolCall{id, name, arguments}`), so PR3 traces and matched benchmarks line up with
/// OpenJarvis (docs Sec. 5, Sec. 8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    /// The result of a tool call, linked by `tool_call_id`.
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    pub at: Timestamp,
    /// Calls the assistant made in this message.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// For `Role::Tool`: which call this answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn new(role: Role, content: String, at: Timestamp) -> Self {
        Self {
            role,
            content,
            at,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }
}

/// A tool the host offers. Field names follow OpenJarvis's `ToolSpec`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Lowercase letters, digits and `_`, at most 64 characters.
    pub name: String,
    pub description: String,
    /// JSON Schema for the arguments object. PR3 checks a strict subset: `properties`
    /// with basic `type`s, `required`, and `additionalProperties` (default `false`).
    pub parameters: String,
    /// Consequential tools (sending, paying, sharing) are never run directly: the core
    /// hands their calls to the host as [`crate::Action::Propose`], for the trusted
    /// confirmation path (docs Sec. 4.6).
    #[serde(default)]
    pub requires_confirmation: bool,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
}

/// A tool call as the model proposed it: untrusted, not yet validated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposedToolCall {
    pub name: String,
    /// Raw JSON text as the model wrote it.
    pub arguments: String,
}

/// A validated tool call the core asks the host to run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Canonical JSON object (keys sorted), so identical calls compare equal.
    pub arguments: String,
}

/// What the backend reports about one generation; feeds the trace.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationInfo {
    pub model: String,
    pub engine: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}
