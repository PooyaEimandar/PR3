//! PR3's wire contract, version 0 (docs Sec. 4.1, Sec. 4.2): what a phone says to its node and
//! what the node answers. The node is the user's own computer, lending its larger model;
//! model borrowing leaves storage and the trusted side on the phone. Continuity is
//! enabled by pairing and shares selected records, never approval authority (Sec. 6.3).
//!
//! Messages travel as CBOR inside an authenticated, encrypted session (`pr3-hybrid`).
//! The types are `no_std`; encoding needs the `std` feature.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::disallowed_macros))] // tests may assert (clippy.toml)

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use pr3_core::{GenerationInfo, InferenceRequest, ProposedToolCall};
use serde::{Deserialize, Serialize};

/// This crate's protocol version. Peers with a different version refuse to talk.
pub const VERSION: u16 = 0;

/// The largest message either side accepts, after decryption.
pub const MAX_MESSAGE_BYTES: usize = 1 << 20;

/// Selected records carried inside the paired, encrypted transport.
/// No approval, capability, vault key or action-execution request crosses here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuityChange {
    pub sequence: u64,
    pub id: String,
    pub kind: String,
    pub owner: String,
    pub epoch: u64,
    pub revision: u64,
    /// None is a permanent deletion tombstone. Payloads are validated by the store.
    pub payload: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuityPage {
    pub changes: Vec<ContinuityChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuityRequest {
    pub version: u16,
    /// Last node sequence durably applied by the phone.
    pub cursor: u64,
    pub page: ContinuityPage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuityReply {
    /// Last phone sequence durably applied by the node.
    pub acknowledged: u64,
    pub page: ContinuityPage,
}

/// From a paired phone to its node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToNode {
    /// First message of every session.
    Hello {
        version: u16,
        device: String,
    },
    /// Run the model on this request and stream the answer.
    Generate {
        id: u64,
        request: InferenceRequest,
    },
    /// Stop generating for request `id`; the node answers `Failed`.
    Cancel {
        id: u64,
    },
    Continuity {
        request: ContinuityRequest,
    },
}

/// From a node to a paired phone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromNode {
    /// Answer to `Hello`: the node's name and the model it lends.
    Hello {
        version: u16,
        node: String,
        model: String,
    },
    /// Visible text as the model writes it.
    Token {
        id: u64,
        text: String,
    },
    /// The finished answer. Tool calls are proposals: the phone's core validates them.
    Done {
        id: u64,
        generated: Generated,
    },
    Failed {
        id: u64,
        reason: String,
    },
    Continuity {
        reply: ContinuityReply,
    },
}

/// One model answer, as the phone's host needs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Generated {
    pub content: String,
    pub tool_calls: Vec<ProposedToolCall>,
    pub info: GenerationInfo,
}

#[cfg(feature = "std")]
pub use cbor::{DecodeError, decode, encode};

#[cfg(feature = "std")]
mod cbor {
    use super::{MAX_MESSAGE_BYTES, Vec};
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DecodeError(pub String);

    impl std::fmt::Display for DecodeError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "malformed message: {}", self.0)
        }
    }

    impl std::error::Error for DecodeError {}

    /// Encodes a message as CBOR. Messages of this crate's types always encode.
    pub fn encode<T: Serialize>(message: &T) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Writing these types to a Vec cannot fail.
        let _ = ciborium::into_writer(message, &mut bytes);
        bytes
    }

    /// Decodes untrusted bytes; anything malformed or oversized is an error.
    pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, DecodeError> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(DecodeError("too large".into()));
        }
        ciborium::from_reader(bytes).map_err(|e| DecodeError(e.to_string()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use pr3_core::{Message, RequestId, Role, SessionId, Timestamp};

    fn request() -> InferenceRequest {
        InferenceRequest {
            id: RequestId(7),
            session: SessionId(1),
            messages: alloc::vec![Message::new(
                Role::User,
                String::from("What is 2+2?"),
                Timestamp(1)
            )],
            tools: Vec::new(),
        }
    }

    #[test]
    fn messages_round_trip_through_cbor() {
        let sent = ToNode::Generate {
            id: 3,
            request: request(),
        };
        assert_eq!(decode::<ToNode>(&encode(&sent)).unwrap(), sent);
        let answer = FromNode::Done {
            id: 3,
            generated: Generated {
                content: String::from("4"),
                tool_calls: alloc::vec![ProposedToolCall {
                    name: String::from("calculate"),
                    arguments: String::from(r#"{"expression":"2+2"}"#),
                }],
                info: GenerationInfo::default(),
            },
        };
        assert_eq!(decode::<FromNode>(&encode(&answer)).unwrap(), answer);
    }

    #[test]
    fn junk_and_oversized_input_is_refused() {
        assert!(decode::<ToNode>(&[0xff, 0x00, 0x13]).is_err());
        assert!(
            decode::<ToNode>(&encode(&FromNode::Failed {
                id: 1,
                reason: String::new()
            }))
            .is_err()
        );
        assert!(decode::<ToNode>(&alloc::vec![0u8; MAX_MESSAGE_BYTES + 1]).is_err());
    }
}
