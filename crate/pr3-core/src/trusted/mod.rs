//! The trusted side (docs Sec. 4.1, Sec. 4.6, Sec. 7.2): structured intents, a deterministic
//! policy, signed approval receipts, one-use capabilities and a hash-chained audit log.
//!
//! These modules depend on nothing in the agent loop, the tool registry or inference
//! types — only on `Timestamp` — so they can move behind a platform-protected boundary
//! later without an API change (Sec. 7.3). CI checks the direction
//! (`script/check-trusted-side.sh`).

mod audit;
mod gate;
mod hash;
mod intent;
mod policy;
mod receipt;

pub use audit::{Broken, Chain, Entry, Event, verify};
pub use gate::{ApprovalRequest, Gate, GateError, Submitted};
pub use hash::Hash;
pub use intent::{
    Contact, Directory, Envelope, Intent, MAIL_SEND, MAX_BODY_BYTES, MAX_SUBJECT_BYTES, Mail,
    Proposal, Refusal,
};
pub use policy::{Limits, Risk};
pub use receipt::{ApprovalKey, Capability, Object, Receipt, SignedReceipt};
