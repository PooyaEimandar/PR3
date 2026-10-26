//! SHA-256 hashes, written as lowercase hex.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Write as _};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use sha2::{Digest as _, Sha256};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Hash(pub [u8; 32]);

impl Hash {
    /// SHA-256 of the parts, each prefixed with its length, so different parts can
    /// never run together into the same input.
    pub fn of(parts: &[&[u8]]) -> Self {
        let mut hasher = Sha256::new();
        for part in parts {
            let len = u64::try_from(part.len()).unwrap_or(u64::MAX);
            hasher.update(len.to_be_bytes());
            hasher.update(part);
        }
        Self(hasher.finalize().into())
    }

    pub fn to_hex(&self) -> String {
        to_hex(&self.0)
    }

    pub fn from_hex(text: &str) -> Option<Self> {
        from_hex(text)?.try_into().ok().map(Self)
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({})", self.to_hex())
    }
}

impl Serialize for Hash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::from_hex(&text).ok_or_else(|| de::Error::custom("expected 64 hex digits"))
    }
}

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // Writing to a String cannot fail.
        let _ = write!(text, "{b:02x}");
    }
    text
}

pub(crate) fn from_hex(text: &str) -> Option<Vec<u8>> {
    let (pairs, rest) = text.as_bytes().as_chunks::<2>();
    if !rest.is_empty() {
        return None;
    }
    let digit = |c: u8| {
        char::from(c)
            .to_digit(16)
            .and_then(|d| u8::try_from(d).ok())
    };
    pairs
        .iter()
        .map(|[hi, lo]| Some((digit(*hi)? << 4) | digit(*lo)?))
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_and_rejects_junk() {
        let h = Hash::of(&[b"pr3"]);
        assert_eq!(Hash::from_hex(&h.to_hex()), Some(h));
        assert_eq!(Hash::from_hex("abc"), None);
        assert_eq!(Hash::from_hex(&"zz".repeat(32)), None);
        assert_eq!(from_hex("00ff10"), Some(alloc::vec![0, 255, 16]));
    }

    #[test]
    fn parts_cannot_run_together() {
        assert_ne!(Hash::of(&[b"ab", b"c"]), Hash::of(&[b"a", b"bc"]));
    }
}
