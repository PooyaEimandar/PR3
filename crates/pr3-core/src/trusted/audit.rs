//! The audit log (Roadmap §4.1, §4.6 step 8): append-only and hash-chained. Each entry
//! hashes its predecessor, so changing, removing or reordering any entry breaks every
//! hash after it. Signed checkpoints, which also catch a log cut short at the end, come
//! with the device key's platform storage.
//!
//! Entries hold addresses and hashes, not message text: the log proves what was
//! approved and sent without keeping another copy of it.

use alloc::string::String;

use serde::{Deserialize, Serialize};

use super::hash::Hash;
use super::policy::Risk;
use super::receipt::Object;
use crate::types::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// A proposal reached the trusted side.
    Proposed {
        request: String,
        actor: String,
        principal: String,
        operation: String,
    },
    /// The validator refused it.
    Refused { request: String, reason: String },
    /// The policy denied it.
    Denied {
        request: String,
        object: Object,
        reason: String,
    },
    /// The policy asked for the user's approval.
    AwaitingApproval {
        request: String,
        object: Object,
        risk: Risk,
        expires_at: Timestamp,
    },
    /// A correctly signed receipt for exactly the requested object.
    Approved { request: String, receipt: Hash },
    /// The user declined, or the approval could not be accepted.
    Declined { request: String, reason: String },
    /// The approval or capability ran out before it was used.
    Expired { request: String },
    Issued {
        request: String,
        expires_at: Timestamp,
    },
    /// The execution adapter used the capability.
    Redeemed { request: String },
    Executed {
        request: String,
        success: bool,
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub seq: u64,
    pub at: Timestamp,
    /// The previous entry's hash; zero for the first entry.
    pub prev: Hash,
    pub event: Event,
    pub hash: Hash,
}

const AUDIT_DOMAIN: &[u8] = b"pr3-audit-v1";

impl Entry {
    fn hash_of(seq: u64, at: Timestamp, prev: &Hash, event: &Event) -> Hash {
        // Strings, numbers and hashes always serialize; nothing here can fail.
        let event = serde_json::to_vec(event).unwrap_or_default();
        Hash::of(&[
            AUDIT_DOMAIN,
            &seq.to_be_bytes(),
            &at.0.to_be_bytes(),
            &prev.0,
            &event,
        ])
    }
}

/// Where a log stopped verifying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Broken {
    /// The first entry, by position, that does not follow from the ones before it.
    pub at_index: usize,
}

/// Checks that `entries` is one unbroken chain from its first entry.
pub fn verify(entries: &[Entry]) -> Result<(), Broken> {
    let mut prev = Hash::default();
    for (index, (expected_seq, entry)) in (0u64..).zip(entries).enumerate() {
        let consistent = entry.seq == expected_seq
            && entry.prev == prev
            && entry.hash == Entry::hash_of(entry.seq, entry.at, &entry.prev, &entry.event);
        if !consistent {
            return Err(Broken { at_index: index });
        }
        prev = entry.hash;
    }
    Ok(())
}

/// The end of the chain, where the next entry is appended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chain {
    next_seq: u64,
    head: Hash,
}

impl Chain {
    /// Continues a stored log after verifying it.
    pub fn resume(entries: &[Entry]) -> Result<Self, Broken> {
        verify(entries)?;
        Ok(entries.last().map_or_else(Self::default, |last| Self {
            next_seq: last.seq.saturating_add(1),
            head: last.hash,
        }))
    }

    pub fn append(&mut self, at: Timestamp, event: Event) -> Entry {
        let entry = Entry {
            seq: self.next_seq,
            at,
            prev: self.head,
            hash: Entry::hash_of(self.next_seq, at, &self.head, &event),
            event,
        };
        self.next_seq = self.next_seq.saturating_add(1);
        self.head = entry.hash;
        entry
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::ToString;
    use alloc::vec::Vec;
    use proptest::prelude::*;

    fn log(n: usize) -> Vec<Entry> {
        let mut chain = Chain::default();
        (0..n)
            .map(|i| {
                chain.append(
                    Timestamp(1_000 + i as u64),
                    Event::Executed {
                        request: format!("r{i}"),
                        success: i % 2 == 0,
                        detail: "sent".to_string(),
                    },
                )
            })
            .collect()
    }

    #[test]
    fn a_resumed_chain_continues_where_it_stopped() {
        let mut entries = log(3);
        let mut chain = Chain::resume(&entries).unwrap();
        entries.push(chain.append(
            Timestamp(9),
            Event::Redeemed {
                request: "r3".to_string(),
            },
        ));
        assert_eq!(verify(&entries), Ok(()));
        assert_eq!(Chain::resume(&[]), Ok(Chain::default()));
    }

    proptest! {
        /// Any change to any field of any entry, or removing or swapping entries,
        /// breaks verification (Roadmap §12).
        #[test]
        fn any_rewrite_breaks_the_chain(n in 2usize..8, pick in 0usize..8, how in 0u8..7) {
            let mut entries = log(n);
            let i = pick % n;
            match how {
                0 => entries[i].seq += 1,
                1 => entries[i].at = Timestamp(entries[i].at.0 + 1),
                2 => entries[i].prev.0[0] ^= 1,
                3 => entries[i].hash.0[31] ^= 1,
                4 => if let Event::Executed { success, .. } = &mut entries[i].event {
                    *success = !*success;
                },
                5 => { entries.remove(i); if i == n - 1 { return Ok(()); } }
                _ => entries.swap(i, (i + 1) % n),
            }
            prop_assert!(verify(&entries).is_err());
        }
    }
}
