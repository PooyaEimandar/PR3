//! The trusted side under arbitrary proposals, approvals, receipts and audit logs. It
//! never panics; it issues a capability only for a receipt the device key signed
//! exactly as the gate approved it, once per request; and each capability is redeemed
//! at most once.

#![no_main]

use std::collections::HashSet;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use p256::ecdsa::signature::Signer as _;
use p256::ecdsa::{Signature, SigningKey};
use pr3_core::Timestamp;
use pr3_core::trusted::{
    ApprovalKey, Capability, Contact, Directory, Entry, Envelope, Gate, Limits, MAIL_SEND,
    Proposal, Receipt, SignedReceipt, Submitted,
};

#[derive(Arbitrary, Debug)]
struct Case {
    /// An audit log to resume from, as JSON; most inputs are not one.
    log: String,
    ops: Vec<Op>,
}

#[derive(Arbitrary, Debug)]
enum Op {
    Submit {
        mail: bool,
        operation: String,
        arguments: String,
        nonce: String,
        age_ms: u16,
    },
    Approve(u8),
    Decline(u8, String),
    Issue(u8),
    Forge {
        receipt: u8,
        signature: String,
    },
    Alter {
        receipt: u8,
        field: u8,
    },
    Redeem(u8),
    Wait(u16),
}

fn pick<T>(items: &[T], i: u8) -> Option<&T> {
    if items.is_empty() {
        None
    } else {
        items.get(usize::from(i) % items.len())
    }
}

fn sign(key: &SigningKey, receipt: Receipt) -> SignedReceipt {
    let signature: Signature = key.sign(&receipt.signing_bytes());
    let hex = signature
        .to_der()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    SignedReceipt {
        receipt,
        signature: hex,
    }
}

fuzz_target!(|case: Case| {
    let key = SigningKey::from_bytes(&[7u8; 32].into()).unwrap();
    let public = key.verifying_key().to_sec1_point(false);
    let approver = ApprovalKey::from_sec1(public.as_bytes()).unwrap();
    let log: Vec<Entry> = serde_json::from_str(&case.log).unwrap_or_default();
    let mut gate = match Gate::new(Limits::default(), approver.clone(), "mail:sandbox", &log) {
        Ok(gate) => gate,
        Err(_) => Gate::new(Limits::default(), approver, "mail:sandbox", &[]).unwrap(),
    };
    let directory = Directory {
        contacts: vec![Contact {
            name: "Alex Rivera".to_string(),
            address: "alex@example.com".to_string(),
        }],
    };
    let mut now = 10_000_000u64;
    let mut requests: Vec<String> = Vec::new();
    let mut receipts: Vec<Receipt> = Vec::new();
    let mut issued: HashSet<String> = HashSet::new();
    let mut capabilities: Vec<Capability> = Vec::new();
    let mut redeemed: HashSet<String> = HashSet::new();
    for op in case.ops {
        let at = Timestamp(now);
        match op {
            Op::Submit {
                mail,
                operation,
                arguments,
                nonce,
                age_ms,
            } => {
                let envelope = Envelope {
                    actor: "agent:pr3".to_string(),
                    principal: "user:local".to_string(),
                    nonce,
                    received_at: Timestamp(now.saturating_sub(u64::from(age_ms))),
                    proposal: Proposal {
                        operation: if mail {
                            MAIL_SEND.to_string()
                        } else {
                            operation
                        },
                        arguments,
                    },
                };
                if let Submitted::NeedsApproval(request) = gate.submit(&envelope, &directory, at) {
                    requests.push(request.request);
                }
            }
            Op::Approve(i) => {
                if let Some(request) = pick(&requests, i) {
                    if let Ok(receipt) = gate.approve(request, at) {
                        receipts.push(receipt);
                    }
                }
            }
            Op::Decline(i, reason) => {
                if let Some(request) = pick(&requests, i) {
                    let _ = gate.decline(request, &reason, at);
                }
            }
            Op::Issue(i) => {
                if let Some(receipt) = pick(&receipts, i) {
                    if let Ok(capability) = gate.issue(&sign(&key, receipt.clone()), at) {
                        assert!(
                            issued.insert(receipt.request.clone()),
                            "two capabilities for one approval"
                        );
                        capabilities.push(capability);
                    }
                }
            }
            Op::Forge { receipt, signature } => {
                if let Some(receipt) = pick(&receipts, receipt) {
                    let forged = SignedReceipt {
                        receipt: receipt.clone(),
                        signature,
                    };
                    assert!(
                        gate.issue(&forged, at).is_err(),
                        "a capability for a signature the device key did not make"
                    );
                }
            }
            Op::Alter { receipt, field } => {
                if let Some(receipt) = pick(&receipts, receipt) {
                    let mut altered = receipt.clone();
                    match field % 5 {
                        0 => altered.request.push('x'),
                        1 => altered.actor.push('x'),
                        2 => altered.provider.push('x'),
                        3 => altered.expires_at.0 += 1,
                        _ => altered.operation.push('x'),
                    }
                    assert!(
                        gate.issue(&sign(&key, altered), at).is_err(),
                        "a capability for a receipt the gate never approved"
                    );
                }
            }
            Op::Redeem(i) => {
                if let Some(capability) = pick(&capabilities, i) {
                    if gate.redeem(capability, at).is_ok() {
                        assert!(
                            redeemed.insert(capability.id.clone()),
                            "a capability redeemed twice"
                        );
                    }
                }
            }
            Op::Wait(ms) => now += u64::from(ms) * 10,
        }
    }
});
