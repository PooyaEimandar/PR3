//! The policy decision (Roadmap §4.6 steps 3–4, §9.1): deterministic rules over a
//! validated intent and limits the user sets. A model never decides.

use alloc::format;
use alloc::string::{String, ToString};

use serde::{Deserialize, Serialize};

use super::intent::Intent;

/// Limits the user configures; never inferred from conversation (§9.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    /// The kill switch: when false, every consequential action is denied.
    pub enabled: bool,
    pub max_recipients: usize,
    /// Actions in any 24 hours, counting those still waiting for approval.
    pub max_per_day: usize,
    /// How long the user has to approve, in milliseconds.
    pub approval_ms: u64,
    /// How long an issued capability can be used, in milliseconds.
    pub capability_ms: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            enabled: true,
            max_recipients: 3,
            max_per_day: 20,
            approval_ms: 120_000,
            capability_ms: 30_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Low,
    /// More than one recipient: a mistake reaches more people.
    High,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Deny(String),
    /// Ask the user on the trusted confirmation screen. Nothing is sent in the user's
    /// name without it (§7.1); the spike has no pre-approved scopes, so no intent is
    /// allowed outright.
    Confirm(Risk),
}

/// What the policy knows besides the intent, supplied by the trusted side's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    pub actions_last_day: usize,
    pub awaiting_approval: bool,
}

pub fn decide(intent: &Intent, limits: &Limits, context: Context) -> Decision {
    if !limits.enabled {
        return Decision::Deny("consequential actions are switched off".to_string());
    }
    // One question at a time, so the user is never asked to approve in a hurry.
    if context.awaiting_approval {
        return Decision::Deny("another action is waiting for the user's approval".to_string());
    }
    if context.actions_last_day >= limits.max_per_day {
        return Decision::Deny(format!(
            "the limit of {} actions a day is reached",
            limits.max_per_day
        ));
    }
    match intent {
        Intent::SendMail(mail) => {
            if mail.to.len() > limits.max_recipients {
                return Decision::Deny(format!(
                    "at most {} recipients per email",
                    limits.max_recipients
                ));
            }
            Decision::Confirm(if mail.to.len() > 1 {
                Risk::High
            } else {
                Risk::Low
            })
        }
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use crate::trusted::intent::{Contact, Mail};
    use alloc::vec::Vec;

    fn mail_to(n: usize) -> Intent {
        let to: Vec<Contact> = (0..n)
            .map(|i| Contact {
                name: format!("C{i}"),
                address: format!("c{i}@example.com"),
            })
            .collect();
        Intent::SendMail(Mail {
            to,
            subject: "s".to_string(),
            body: "b".to_string(),
        })
    }

    const QUIET: Context = Context {
        actions_last_day: 0,
        awaiting_approval: false,
    };

    #[test]
    fn every_email_needs_approval_and_risk_grows_with_recipients() {
        let limits = Limits::default();
        assert_eq!(
            decide(&mail_to(1), &limits, QUIET),
            Decision::Confirm(Risk::Low)
        );
        assert_eq!(
            decide(&mail_to(3), &limits, QUIET),
            Decision::Confirm(Risk::High)
        );
        assert!(matches!(
            decide(&mail_to(4), &limits, QUIET),
            Decision::Deny(_)
        ));
    }

    #[test]
    fn kill_switch_daily_limit_and_one_question_at_a_time() {
        let off = Limits {
            enabled: false,
            ..Limits::default()
        };
        let denied = |limits: &Limits, context| match decide(&mail_to(1), limits, context) {
            Decision::Deny(why) => why,
            other => panic!("{other:?}"),
        };
        assert!(denied(&off, QUIET).contains("switched off"));
        let busy = Context {
            awaiting_approval: true,
            ..QUIET
        };
        assert!(denied(&Limits::default(), busy).contains("waiting"));
        let spent = Context {
            actions_last_day: 20,
            ..QUIET
        };
        assert!(denied(&Limits::default(), spent).contains("a day"));
    }
}
