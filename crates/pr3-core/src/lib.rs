//! PR3 core: a sans-I/O state machine.
//!
//! The host owns the loop. It feeds [`Input`]s into [`Core::step`] and performs the
//! [`Action`]s that come back — running a model, showing a reply — then feeds the
//! results back in. The core has no clock, entropy, I/O, threads or async runtime of
//! its own (Roadmap §3, §4.2), so the same code runs in apps, browsers and
//! `no_std` targets.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

extern crate alloc;

mod error;
mod event;
mod runtime;
mod types;

pub use crate::error::CoreError;
pub use crate::event::{Action, InferenceRequest, Input};
pub use crate::runtime::{Config, Core};
pub use crate::types::{Message, RequestId, Role, SessionId, Timestamp};
