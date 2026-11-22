//! PR3 core: a sans-I/O state machine.
//!
//! The host owns the loop. It feeds [`Input`]s into [`Core::step`] and performs the
//! [`Action`]s that come back — running a model, showing a reply — then feeds the
//! results back in. [`Core::step_view`] is the same step without copies, in the borrowed
//! forms of [`view`]. The core has no clock, entropy, I/O, threads or async runtime of
//! its own (docs Sec. 3, Sec. 4.2), so the same code runs in apps, browsers and
//! `no_std` targets.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
#![cfg_attr(test, allow(clippy::disallowed_macros))] // tests may assert (clippy.toml)

extern crate alloc;

mod error;
mod event;
mod history;
mod loop_guard;
mod owned;
mod runtime;
mod text;
mod tools;
mod trace;
pub mod trusted;
mod types;

pub use crate::error::CoreError;
pub use crate::owned::{Action, GenerationInfo, InferenceRequest, Input, ProposedToolCall};
pub use crate::runtime::{AFTER_UNTRUSTED, Config, Core, tool_round_budget};
pub use crate::trace::{Outcome, StepType, Trace, TraceStep};
pub use crate::types::{Message, RequestId, Role, SessionId, Timestamp, ToolCall, ToolSpec};

/// The step interface borrowed both ways, for [`Core::step_view`]: an input borrows the
/// host's buffers for one call, and the actions borrow the core until the next call that
/// changes it, so nothing is copied to cross the boundary.
pub mod view {
    pub use crate::event::{Action, InferenceRequest, Input, MessageView};
    pub use crate::runtime::Actions;
    pub use crate::trace::TraceView;
    pub use crate::types::{GenerationInfo, ProposedToolCall};
}
