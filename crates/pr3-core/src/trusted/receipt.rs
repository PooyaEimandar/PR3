//! Approval receipts and one-use capabilities (docs Sec. 7.2 item 5).
//!
//! The user approves an exact [`Object`]; the device's approval key signs a
//! [`Receipt`] binding it to principal, actor, operation, provider, request and
//! expiry. A valid receipt is exchanged for a [`Capability`] with the same binding,
//! good for one use within seconds. The execution adapter gets the capability — never
//! a credential.

use alloc::string::String;
use alloc::vec::Vec;

use p256::ecdsa::signature::Verifier as _;
use p256::ecdsa::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

use super::hash::{Hash, from_hex};
use super::intent::Intent;
use crate::types::Timestamp;

/// Exactly what is approved. For an email: every recipient's address and the hashes
/// of the subject and body, so changing any of them needs a new approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Object {
    Mail {
        to: Vec<String>,
        subject: Hash,
        body: Hash,
    },
}

impl Object {
    pub fn of(intent: &Intent) -> Self {
        match intent {
            Intent::SendMail(mail) => Self::Mail {
                to: mail.to.iter().map(|c| c.address.clone()).collect(),
                subject: Hash::of(&[mail.subject.as_bytes()]),
                body: Hash::of(&[mail.body.as_bytes()]),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    /// The request approved: its envelope's nonce.
    pub request: String,
    pub principal: String,
    pub actor: String,
    pub operation: String,
    /// Who executes it, e.g. `mail:sandbox`.
    pub provider: String,
    pub object: Object,
    pub approved_at: Timestamp,
    /// The approval is worthless after this.
    pub expires_at: Timestamp,
}

/// Separates receipt signatures from anything else the key might ever sign.
const RECEIPT_DOMAIN: &[u8] = b"pr3-approval-receipt-v1\n";

impl Receipt {
    /// The bytes the approval key signs: a domain tag and the receipt as JSON, whose
    /// field order is fixed by this struct.
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = RECEIPT_DOMAIN.to_vec();
        // Strings, numbers and lists always serialize; nothing here can fail.
        bytes.extend(serde_json::to_vec(self).unwrap_or_default());
        bytes
    }

    pub fn hash(&self) -> Hash {
        Hash::of(&[&self.signing_bytes()])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedReceipt {
    pub receipt: Receipt,
    /// ECDSA P-256 with SHA-256 over [`Receipt::signing_bytes`], in hex: DER as Secure
    /// Enclave and Android Keystore return it, or 64 raw bytes.
    pub signature: String,
}

/// The public half of the device key that approves actions (Sec. 6.3). Its private half
/// stays in `pr3-vault`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalKey(VerifyingKey);

impl ApprovalKey {
    /// From an uncompressed or compressed SEC1 point, as platform keystores export it.
    pub fn from_sec1(bytes: &[u8]) -> Option<Self> {
        VerifyingKey::from_sec1_bytes(bytes).ok().map(Self)
    }

    pub fn verifies(&self, signed: &SignedReceipt) -> bool {
        let Some(bytes) = from_hex(&signed.signature) else {
            return false;
        };
        let Ok(signature) = Signature::from_der(&bytes).or_else(|_| Signature::from_slice(&bytes))
        else {
            return false;
        };
        self.0
            .verify(&signed.receipt.signing_bytes(), &signature)
            .is_ok()
    }
}

/// Released for one approved request; what the execution adapter receives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    /// The request it was issued for.
    pub id: String,
    pub principal: String,
    pub actor: String,
    pub operation: String,
    pub provider: String,
    pub object: Object,
    /// The receipt it was exchanged for.
    pub receipt: Hash,
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
    pub max_uses: u32,
}
