//! The trusted side's state (Roadmap §4.6): each consequential action goes proposed →
//! validated → decided → approved → capability → executed, and every step lands in the
//! audit log. Sans-I/O like the agent core: the host supplies the time, the envelope's
//! nonce, the user's answer and the vault's signature, and stores the audit entries
//! this hands back.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use serde::Serialize;

use super::audit::{Broken, Chain, Entry, Event};
use super::intent::{
    self, Directory, Envelope, Intent, MAX_NONCE_BYTES, Mail, ReplayWindow, ValidIntent,
};
use super::policy::{self, Context, Decision, Limits, Risk};
use super::receipt::{ApprovalKey, Capability, Object, Receipt, SignedReceipt};
use crate::types::Timestamp;

const DAY_MS: u64 = 24 * 60 * 60 * 1000;
/// Longest result detail kept in the audit log.
const MAX_DETAIL: usize = 256;

/// What the user is asked to approve. Built from the validated intent — the directory's
/// addresses and the bounded text — never from anything the model wrote for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApprovalRequest {
    pub request: String,
    pub operation: String,
    pub provider: String,
    pub risk: Risk,
    pub mail: Mail,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submitted {
    /// The proposal is malformed or refers to something that does not exist.
    Refused(String),
    /// Valid, but the policy says no.
    Denied(String),
    /// Ask the user.
    NeedsApproval(ApprovalRequest),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateError {
    /// No such request is waiting, or it was already answered.
    UnknownRequest,
    /// The approval or capability ran out.
    Expired,
    NotApproved,
    /// The signed receipt is not exactly the one this request needs.
    Mismatch,
    BadSignature,
    /// The capability was used already, or never issued.
    NotIssued,
}

impl fmt::Display for GateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnknownRequest => "no such request is waiting for approval",
            Self::Expired => "the approval expired",
            Self::NotApproved => "the user has not approved this request",
            Self::Mismatch => "the approval does not match the request",
            Self::BadSignature => "the approval's signature is not valid",
            Self::NotIssued => "the capability was already used or never issued",
        })
    }
}

#[derive(Debug)]
struct Waiting {
    intent: ValidIntent,
    object: Object,
    expires_at: Timestamp,
    approved_at: Option<Timestamp>,
}

#[derive(Debug)]
struct Issued {
    capability: Capability,
    intent: Intent,
}

#[derive(Debug)]
pub struct Gate {
    limits: Limits,
    approver: ApprovalKey,
    provider: String,
    replay: ReplayWindow,
    waiting: BTreeMap<String, Waiting>,
    issued: BTreeMap<String, Issued>,
    /// When recent capabilities were issued, for the daily limit.
    recent: VecDeque<Timestamp>,
    chain: Chain,
    unsaved: Vec<Entry>,
}

impl Gate {
    /// Picks up where `log` (the stored audit log) left off, after verifying it.
    pub fn new(
        limits: Limits,
        approver: ApprovalKey,
        provider: &str,
        log: &[Entry],
    ) -> Result<Self, Broken> {
        let chain = Chain::resume(log)?;
        let recent = log
            .iter()
            .filter(|e| matches!(e.event, Event::Issued { .. }))
            .map(|e| e.at)
            .collect();
        Ok(Self {
            limits,
            approver,
            provider: provider.to_string(),
            replay: ReplayWindow::default(),
            waiting: BTreeMap::new(),
            issued: BTreeMap::new(),
            recent,
            chain,
            unsaved: Vec::new(),
        })
    }

    /// Validates a proposal and decides on it.
    pub fn submit(
        &mut self,
        envelope: &Envelope,
        directory: &Directory,
        now: Timestamp,
    ) -> Submitted {
        self.expire(now);
        let request: String = envelope.nonce.chars().take(MAX_NONCE_BYTES).collect();
        self.log(
            now,
            Event::Proposed {
                request: request.clone(),
                actor: envelope.actor.clone(),
                principal: envelope.principal.clone(),
                operation: envelope.proposal.operation.chars().take(64).collect(),
            },
        );
        let valid = match intent::validate(envelope, directory, &mut self.replay, now) {
            Ok(valid) => valid,
            Err(refusal) => {
                let reason = refusal.to_string();
                self.log(
                    now,
                    Event::Refused {
                        request,
                        reason: reason.clone(),
                    },
                );
                return Submitted::Refused(reason);
            }
        };
        let object = Object::of(&valid.intent);
        let context = Context {
            actions_last_day: self.actions_last_day(now),
            awaiting_approval: !self.waiting.is_empty(),
        };
        match policy::decide(&valid.intent, &self.limits, context) {
            Decision::Deny(reason) => {
                self.log(
                    now,
                    Event::Denied {
                        request,
                        object,
                        reason: reason.clone(),
                    },
                );
                Submitted::Denied(reason)
            }
            Decision::Confirm(risk) => {
                let expires_at = Timestamp(now.0.saturating_add(self.limits.approval_ms));
                self.log(
                    now,
                    Event::AwaitingApproval {
                        request: request.clone(),
                        object: object.clone(),
                        risk,
                        expires_at,
                    },
                );
                let Intent::SendMail(mail) = &valid.intent;
                let shown = ApprovalRequest {
                    request: request.clone(),
                    operation: intent::MAIL_SEND.to_string(),
                    provider: self.provider.clone(),
                    risk,
                    mail: mail.clone(),
                    expires_at,
                };
                self.waiting.insert(
                    request,
                    Waiting {
                        intent: valid,
                        object,
                        expires_at,
                        approved_at: None,
                    },
                );
                Submitted::NeedsApproval(shown)
            }
        }
    }

    /// The user approved on the trusted screen: returns the receipt for the vault to
    /// sign. Asking again returns the same receipt.
    pub fn approve(&mut self, request: &str, now: Timestamp) -> Result<Receipt, GateError> {
        self.expire(now);
        let provider = self.provider.clone();
        let waiting = self
            .waiting
            .get_mut(request)
            .ok_or(GateError::UnknownRequest)?;
        let approved_at = *waiting.approved_at.get_or_insert(now);
        Ok(receipt_for(request, waiting, &provider, approved_at))
    }

    /// The user declined, or the host gave up waiting.
    pub fn decline(
        &mut self,
        request: &str,
        reason: &str,
        now: Timestamp,
    ) -> Result<(), GateError> {
        self.expire(now);
        self.waiting
            .remove(request)
            .ok_or(GateError::UnknownRequest)?;
        self.log(
            now,
            Event::Declined {
                request: request.to_string(),
                reason: reason.to_string(),
            },
        );
        Ok(())
    }

    /// Exchanges a signed receipt for a one-use capability. The receipt must be exactly
    /// the one [`Gate::approve`] returned for a request still waiting, signed by the
    /// approval key. Any failure ends the request: an approval is never retried.
    pub fn issue(
        &mut self,
        signed: &SignedReceipt,
        now: Timestamp,
    ) -> Result<Capability, GateError> {
        self.expire(now);
        let request = signed.receipt.request.clone();
        let waiting = self
            .waiting
            .remove(&request)
            .ok_or(GateError::UnknownRequest)?;
        let checked = match waiting.approved_at {
            None => Err(GateError::NotApproved),
            Some(at) if signed.receipt != receipt_for(&request, &waiting, &self.provider, at) => {
                Err(GateError::Mismatch)
            }
            Some(_) if !self.approver.verifies(signed) => Err(GateError::BadSignature),
            Some(_) => Ok(()),
        };
        if let Err(error) = checked {
            self.log(
                now,
                Event::Declined {
                    request,
                    reason: error.to_string(),
                },
            );
            return Err(error);
        }
        let receipt = signed.receipt.hash();
        let capability = Capability {
            id: request.clone(),
            principal: waiting.intent.principal.clone(),
            actor: waiting.intent.actor.clone(),
            operation: signed.receipt.operation.clone(),
            provider: self.provider.clone(),
            object: waiting.object,
            receipt,
            issued_at: now,
            expires_at: Timestamp(now.0.saturating_add(self.limits.capability_ms)),
            max_uses: 1,
        };
        self.log(
            now,
            Event::Approved {
                request: request.clone(),
                receipt,
            },
        );
        self.log(
            now,
            Event::Issued {
                request: request.clone(),
                expires_at: capability.expires_at,
            },
        );
        self.recent.push_back(now);
        self.issued.insert(
            request,
            Issued {
                capability: capability.clone(),
                intent: waiting.intent.intent,
            },
        );
        Ok(capability)
    }

    /// The execution adapter presents the capability; it gets back exactly the approved
    /// intent to execute, once.
    pub fn redeem(&mut self, capability: &Capability, now: Timestamp) -> Result<Intent, GateError> {
        self.expire(now);
        let issued = self
            .issued
            .remove(&capability.id)
            .ok_or(GateError::NotIssued)?;
        if issued.capability != *capability {
            self.log(
                now,
                Event::Declined {
                    request: capability.id.clone(),
                    reason: GateError::Mismatch.to_string(),
                },
            );
            return Err(GateError::Mismatch);
        }
        self.log(
            now,
            Event::Redeemed {
                request: capability.id.clone(),
            },
        );
        Ok(issued.intent)
    }

    /// Records what the execution adapter reported.
    pub fn executed(&mut self, request: &str, result: Result<&str, &str>, now: Timestamp) {
        let (success, detail) = match result {
            Ok(detail) => (true, detail),
            Err(detail) => (false, detail),
        };
        self.log(
            now,
            Event::Executed {
                request: request.to_string(),
                success,
                detail: detail.chars().take(MAX_DETAIL).collect(),
            },
        );
    }

    /// Audit entries not yet handed to the host, oldest first.
    pub fn take_log(&mut self) -> Vec<Entry> {
        core::mem::take(&mut self.unsaved)
    }

    /// Ends approvals and capabilities whose time ran out.
    pub fn expire(&mut self, now: Timestamp) {
        let late_approvals: Vec<String> = self
            .waiting
            .iter()
            .filter(|(_, w)| now > w.expires_at)
            .map(|(id, _)| id.clone())
            .collect();
        let late_capabilities: Vec<String> = self
            .issued
            .iter()
            .filter(|(_, i)| now > i.capability.expires_at)
            .map(|(id, _)| id.clone())
            .collect();
        for request in late_approvals {
            self.waiting.remove(&request);
            self.log(now, Event::Expired { request });
        }
        for request in late_capabilities {
            self.issued.remove(&request);
            self.log(now, Event::Expired { request });
        }
    }

    fn actions_last_day(&mut self, now: Timestamp) -> usize {
        while self
            .recent
            .front()
            .is_some_and(|at| now.0.saturating_sub(at.0) >= DAY_MS)
        {
            self.recent.pop_front();
        }
        self.recent.len() + self.waiting.len() + self.issued.len()
    }

    fn log(&mut self, at: Timestamp, event: Event) {
        let entry = self.chain.append(at, event);
        self.unsaved.push(entry);
    }
}

fn receipt_for(
    request: &str,
    waiting: &Waiting,
    provider: &str,
    approved_at: Timestamp,
) -> Receipt {
    Receipt {
        request: request.to_string(),
        principal: waiting.intent.principal.clone(),
        actor: waiting.intent.actor.clone(),
        operation: intent::MAIL_SEND.to_string(),
        provider: provider.to_string(),
        object: waiting.object.clone(),
        approved_at,
        expires_at: waiting.expires_at,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
mod tests {
    use super::*;
    use crate::trusted::audit::verify;
    use crate::trusted::hash::to_hex;
    use crate::trusted::intent::Proposal;
    use crate::trusted::intent::tests::directory;
    use p256::ecdsa::signature::Signer as _;
    use p256::ecdsa::{Signature, SigningKey};
    use proptest::prelude::*;

    const T0: u64 = 1_000_000;

    fn device_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32].into()).unwrap()
    }

    fn sign(receipt: Receipt, key: &SigningKey) -> SignedReceipt {
        let signature: Signature = key.sign(&receipt.signing_bytes());
        SignedReceipt {
            receipt,
            signature: to_hex(signature.to_der().as_bytes()),
        }
    }

    fn gate() -> Gate {
        let public = device_key().verifying_key().to_encoded_point(false);
        let approver = ApprovalKey::from_sec1(public.as_bytes()).unwrap();
        Gate::new(Limits::default(), approver, "mail:sandbox", &[]).unwrap()
    }

    fn envelope(nonce: &str, arguments: &str, at: u64) -> Envelope {
        Envelope {
            actor: "agent:pr3".to_string(),
            principal: "user:local".to_string(),
            nonce: nonce.to_string(),
            received_at: Timestamp(at),
            proposal: Proposal {
                operation: intent::MAIL_SEND.to_string(),
                arguments: arguments.to_string(),
            },
        }
    }

    const TO_ALEX: &str = r#"{"to":"Alex","subject":"Lunch","body":"Noon at the usual place?"}"#;

    /// Submits, approves and signs; returns the signed receipt.
    fn approved(gate: &mut Gate, nonce: &str) -> SignedReceipt {
        let Submitted::NeedsApproval(asked) =
            gate.submit(&envelope(nonce, TO_ALEX, T0), &directory(), Timestamp(T0))
        else {
            panic!("expected an approval request");
        };
        assert_eq!(asked.mail.to[0].address, "alex@example.com");
        let receipt = gate.approve(&asked.request, Timestamp(T0 + 5)).unwrap();
        sign(receipt, &device_key())
    }

    #[test]
    fn approved_mail_is_issued_redeemed_once_and_fully_audited() {
        let mut gate = gate();
        let signed = approved(&mut gate, "r1");
        let capability = gate.issue(&signed, Timestamp(T0 + 6)).unwrap();
        assert_eq!(capability.max_uses, 1);
        assert_eq!(capability.receipt, signed.receipt.hash());
        let Intent::SendMail(mail) = gate.redeem(&capability, Timestamp(T0 + 7)).unwrap();
        assert_eq!(mail.subject, "Lunch");
        gate.executed("r1", Ok("message 1"), Timestamp(T0 + 8));

        assert_eq!(
            gate.redeem(&capability, Timestamp(T0 + 9)),
            Err(GateError::NotIssued)
        );
        assert_eq!(
            gate.issue(&signed, Timestamp(T0 + 9)),
            Err(GateError::UnknownRequest)
        );

        let log = gate.take_log();
        let events: Vec<&str> = log
            .iter()
            .map(|e| match e.event {
                Event::Proposed { .. } => "proposed",
                Event::AwaitingApproval { .. } => "awaiting",
                Event::Approved { .. } => "approved",
                Event::Issued { .. } => "issued",
                Event::Redeemed { .. } => "redeemed",
                Event::Executed { .. } => "executed",
                _ => "other",
            })
            .collect();
        assert_eq!(
            events,
            [
                "proposed", "awaiting", "approved", "issued", "redeemed", "executed"
            ]
        );
        assert_eq!(verify(&log), Ok(()));
    }

    #[test]
    fn declined_refused_and_denied_requests_issue_nothing() {
        let mut gate = gate();
        let refused = gate.submit(
            &envelope(
                "r1",
                r#"{"to":"eve@evil.example","subject":"s","body":"b"}"#,
                T0,
            ),
            &directory(),
            Timestamp(T0),
        );
        assert!(matches!(refused, Submitted::Refused(_)));

        let signed = approved(&mut gate, "r2");
        // A second request while one waits is denied.
        let busy = gate.submit(
            &envelope("r3", TO_ALEX, T0),
            &directory(),
            Timestamp(T0 + 1),
        );
        assert!(matches!(busy, Submitted::Denied(_)));

        gate.decline("r2", "the user declined", Timestamp(T0 + 10))
            .unwrap();
        assert_eq!(
            gate.issue(&signed, Timestamp(T0 + 11)),
            Err(GateError::UnknownRequest)
        );
    }

    #[test]
    fn approval_needs_an_approval_and_the_right_key() {
        let mut gate = gate();
        let Submitted::NeedsApproval(asked) =
            gate.submit(&envelope("r1", TO_ALEX, T0), &directory(), Timestamp(T0))
        else {
            panic!()
        };
        // Signed without the user's approval on the trusted screen.
        let forged = sign(
            Receipt {
                request: asked.request.clone(),
                principal: "user:local".to_string(),
                actor: "agent:pr3".to_string(),
                operation: intent::MAIL_SEND.to_string(),
                provider: "mail:sandbox".to_string(),
                object: Object::of(&Intent::SendMail(asked.mail.clone())),
                approved_at: Timestamp(T0),
                expires_at: asked.expires_at,
            },
            &device_key(),
        );
        assert_eq!(
            gate.issue(&forged, Timestamp(T0 + 1)),
            Err(GateError::NotApproved)
        );

        let mut gate = self::gate();
        let receipt = approved(&mut gate, "r2").receipt;
        let other_key = SigningKey::from_bytes(&[9u8; 32].into()).unwrap();
        let wrong = sign(receipt, &other_key);
        assert_eq!(
            gate.issue(&wrong, Timestamp(T0 + 6)),
            Err(GateError::BadSignature)
        );
    }

    #[test]
    fn the_daily_limit_survives_a_restart() {
        let limits = Limits {
            max_per_day: 1,
            ..Limits::default()
        };
        let mut first = gate();
        first.limits = limits;
        let signed = approved(&mut first, "r1");
        first.issue(&signed, Timestamp(T0 + 6)).unwrap();
        let log = first.take_log();

        let mut again = Gate::new(limits, first.approver.clone(), "mail:sandbox", &log).unwrap();
        let next = again.submit(
            &envelope("r2", TO_ALEX, T0 + 60_000),
            &directory(),
            Timestamp(T0 + 60_000),
        );
        assert!(matches!(next, Submitted::Denied(why) if why.contains("a day")));
        // A day after the first capability was issued.
        let later = T0 + 6 + DAY_MS;
        let next_day = again.submit(
            &envelope("r3", TO_ALEX, later),
            &directory(),
            Timestamp(later),
        );
        assert!(matches!(next_day, Submitted::NeedsApproval(_)));
    }

    #[test]
    fn a_broken_log_is_refused() {
        let mut gate = gate();
        approved(&mut gate, "r1");
        let mut log = gate.take_log();
        log[0].at = Timestamp(1);
        let refused = Gate::new(
            Limits::default(),
            gate.approver.clone(),
            "mail:sandbox",
            &log,
        );
        assert_eq!(refused.unwrap_err(), Broken { at_index: 0 });
    }

    proptest! {
        /// No capability without a receipt matching the request exactly and signed by
        /// the approval key (Roadmap §12).
        #[test]
        fn tampered_receipts_never_issue(field in 0u8..9, byte in 0usize..64) {
            let mut gate = gate();
            let mut signed = approved(&mut gate, "r1");
            let r = &mut signed.receipt;
            // Fields 0–7 change what was approved and are signed again with the right
            // key; 8 damages the signature itself.
            match field {
                0 => r.request.push('x'),
                1 => r.principal = "user:other".to_string(),
                2 => r.actor = "agent:other".to_string(),
                3 => r.operation = "mail.forward".to_string(),
                4 => r.provider = "mail:live".to_string(),
                5 => {
                    let Object::Mail { to, .. } = &mut r.object;
                    to.push("eve@evil.example".to_string());
                }
                6 => r.approved_at = Timestamp(r.approved_at.0 + 1),
                7 => r.expires_at = Timestamp(r.expires_at.0 + 60_000),
                _ => {
                    let mut sig = signed.signature.clone().into_bytes();
                    let i = byte % sig.len();
                    sig[i] = if sig[i] == b'0' { b'1' } else { b'0' };
                    signed.signature = String::from_utf8(sig).unwrap();
                }
            }
            if field < 8 {
                signed = sign(signed.receipt, &device_key());
            }
            prop_assert!(gate.issue(&signed, Timestamp(T0 + 6)).is_err());
            prop_assert!(gate.issued.is_empty());
        }

        /// Expired approvals and capabilities never authorize (Roadmap §12).
        #[test]
        fn late_approvals_and_capabilities_never_authorize(extra in 1u64..10_000_000) {
            let limits = Limits::default();
            let mut gate = gate();
            let signed = approved(&mut gate, "r1");
            let late = T0 + limits.approval_ms + extra;
            prop_assert!(gate.issue(&signed, Timestamp(late)).is_err());

            let mut gate = self::gate();
            let signed = approved(&mut gate, "r2");
            let capability = gate.issue(&signed, Timestamp(T0 + 6)).unwrap();
            let late = capability.expires_at.0 + extra;
            prop_assert!(gate.redeem(&capability, Timestamp(late)).is_err());
        }

        /// A capability works only as issued: any altered copy is refused and used up.
        #[test]
        fn altered_capabilities_are_refused(field in 0u8..4) {
            let mut gate = gate();
            let signed = approved(&mut gate, "r1");
            let mut capability = gate.issue(&signed, Timestamp(T0 + 6)).unwrap();
            match field {
                0 => capability.max_uses = 2,
                1 => capability.expires_at = Timestamp(capability.expires_at.0 + 1),
                2 => capability.provider = "mail:live".to_string(),
                _ => capability.receipt.0[0] ^= 1,
            }
            prop_assert_eq!(gate.redeem(&capability, Timestamp(T0 + 7)), Err(GateError::Mismatch));
            prop_assert!(gate.issued.is_empty());
        }
    }
}
