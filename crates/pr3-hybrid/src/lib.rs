//! PR3's private hybrid (docs Sec. 6.3, M2-E2–E4): a phone pairs with the user's own
//! computer, which then lends it its larger model.
//!
//! - **Pairing.** The computer shows a six-digit code, valid once and for two minutes.
//!   The phone and the computer turn it into a strong shared key with SPAKE2, a
//!   password-authenticated key exchange, so a code overheard or guessed offline is
//!   useless; that key then authenticates a Noise `XXpsk3` handshake in which each side
//!   learns the other's long-term public key.
//! - **Sessions.** Every later connection is a Noise `IK` handshake: mutually
//!   authenticated with those keys, forward-secret, and refused by the node for any key
//!   it has not paired with, so forgetting a phone revokes it. Messages are
//!   `pr3-protocol` CBOR, encrypted and in order; nothing can be replayed into another
//!   session.
//! - **Roles.** The node only generates: the phone keeps the conversation, its memory,
//!   its tools and the trusted side, so a node can propose an action but never approve
//!   or run one. When the node cannot be reached, the phone answers on its own.

#![forbid(unsafe_code)]

mod channel;
mod client;
mod keys;
mod node;
mod pairing;

use std::fmt;
use std::io;

pub use client::{NodeClient, pair};
pub use keys::{DeviceKey, Peer};
pub use node::{Generator, Node, PairingCode};

/// The port a node listens on unless configured otherwise.
pub const DEFAULT_PORT: u16 = 7313;

#[derive(Debug)]
pub enum HybridError {
    /// The node could not be reached, or the connection broke.
    Unreachable(io::Error),
    /// The other side refused: an unknown device, a wrong or expired code, another
    /// protocol version.
    Refused(String),
    /// The other side sent something malformed.
    Protocol(String),
    /// The node could not answer, for example because its model failed.
    NodeFailed(String),
    /// The request was cancelled.
    Cancelled,
}

impl fmt::Display for HybridError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreachable(e) => write!(f, "the node cannot be reached: {e}"),
            Self::Refused(why) => write!(f, "refused: {why}"),
            Self::Protocol(why) => write!(f, "protocol error: {why}"),
            Self::NodeFailed(why) => write!(f, "the node failed: {why}"),
            Self::Cancelled => f.write_str("cancelled"),
        }
    }
}

impl std::error::Error for HybridError {}

impl From<io::Error> for HybridError {
    fn from(e: io::Error) -> Self {
        Self::Unreachable(e)
    }
}

impl From<snow::Error> for HybridError {
    fn from(e: snow::Error) -> Self {
        Self::Refused(e.to_string())
    }
}

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn from_hex(text: &str) -> Option<Vec<u8>> {
    let (pairs, rest) = text.as_bytes().as_chunks::<2>();
    if !rest.is_empty() {
        return None;
    }
    pairs
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests;
