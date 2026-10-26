use alloc::string::String;
use core::fmt;

use crate::types::{RequestId, SessionId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    /// Input text is longer than [`crate::Config::max_input_bytes`].
    InputTooLarge { len: usize, max: usize },
    /// Input text is empty after trimming.
    EmptyInput,
    /// The host answered a request the core never made, or answered it twice.
    UnknownRequest(RequestId),
    /// The host reported a tool call the core never issued, or reported it twice.
    UnknownToolCall(String),
    /// The session is still working on the previous message; one turn at a time.
    TurnInProgress(SessionId),
    /// A tool registration was rejected.
    InvalidToolSpec(String),
    /// The session has no turn in progress.
    NoTurn(SessionId),
    /// An allocation at an untrusted boundary failed.
    OutOfMemory,
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge { len, max } => {
                write!(f, "input is {len} bytes; the limit is {max}")
            }
            Self::EmptyInput => f.write_str("input is empty"),
            Self::UnknownRequest(id) => write!(f, "unknown or already answered request {}", id.0),
            Self::UnknownToolCall(id) => write!(f, "unknown or already answered tool call {id}"),
            Self::TurnInProgress(_) => f.write_str("still answering the previous message"),
            Self::InvalidToolSpec(why) => write!(f, "invalid tool: {why}"),
            Self::NoTurn(_) => f.write_str("no turn in progress"),
            Self::OutOfMemory => f.write_str("out of memory"),
        }
    }
}

impl core::error::Error for CoreError {}
