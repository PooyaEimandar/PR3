use core::fmt;

use crate::types::RequestId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreError {
    /// Input text is longer than [`crate::Config::max_input_bytes`].
    InputTooLarge { len: usize, max: usize },
    /// Input text is empty after trimming.
    EmptyInput,
    /// The host answered a request the core never made, or answered it twice.
    UnknownRequest(RequestId),
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
            Self::OutOfMemory => f.write_str("out of memory"),
        }
    }
}

impl core::error::Error for CoreError {}
