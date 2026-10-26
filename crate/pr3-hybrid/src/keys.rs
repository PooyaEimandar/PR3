//! Long-term device keys and the peers a device has paired with.
//!
//! These X25519 keys authenticate the transport between a phone and its node. They are
//! not approval keys: approvals stay with the phone's vault (Secure Enclave, Keystore),
//! and nothing here can sign one.

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::pairing::SESSION_NOISE;
use crate::{from_hex, to_hex};

/// This device's long-term transport key.
pub struct DeviceKey {
    pub(crate) private: [u8; 32],
    pub(crate) public: [u8; 32],
}

impl DeviceKey {
    /// Opens the key at `path`, creating it the first time. The file is readable only by
    /// the app, in its private, backup-excluded data folder.
    pub fn open(path: &Path) -> io::Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => Self::from_private(&from_hex(text.trim()).unwrap_or_default())
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "damaged device key")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let key = Self::generate()?;
                if let Some(dir) = path.parent() {
                    fs::create_dir_all(dir)?;
                }
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
                let mut file = options.open(path)?;
                file.write_all(to_hex(&key.private).as_bytes())?;
                file.sync_all()?;
                Ok(key)
            }
            Err(e) => Err(e),
        }
    }

    pub fn generate() -> io::Result<Self> {
        let params = SESSION_NOISE
            .parse()
            .map_err(|e: snow::Error| io::Error::other(e.to_string()))?;
        let pair = snow::Builder::new(params)
            .generate_keypair()
            .map_err(|e| io::Error::other(e.to_string()))?;
        Self::from_private(&pair.private)
            .filter(|key| key.public[..] == pair.public[..])
            .ok_or_else(|| io::Error::other("key generation failed"))
    }

    fn from_private(private: &[u8]) -> Option<Self> {
        let private: [u8; 32] = private.try_into().ok()?;
        let public = x25519_public(&private)?;
        Some(Self { private, public })
    }

    /// The public key, in hex: what a peer records.
    pub fn public_hex(&self) -> String {
        to_hex(&self.public)
    }
}

/// The X25519 public key for a private key, computed by snow's own implementation.
fn x25519_public(private: &[u8; 32]) -> Option<[u8; 32]> {
    use snow::params::DHChoice;
    use snow::resolvers::{CryptoResolver as _, DefaultResolver};
    let mut dh = DefaultResolver.resolve_dh(&DHChoice::Curve25519)?;
    dh.set(private);
    dh.pubkey().try_into().ok()
}

/// A device on the other end: a node the phone paired with, or a phone the node paired
/// with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peer {
    pub name: String,
    /// The peer's long-term public key, in hex.
    pub public_key: String,
    /// When it was paired, in milliseconds since the Unix epoch.
    pub paired_at: u64,
    /// Where to reach it: set for a node, empty for a phone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
}

impl Peer {
    pub(crate) fn new(name: &str, public: &[u8; 32], address: Option<String>) -> Self {
        let paired_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        Self {
            name: clean_name(name),
            public_key: to_hex(public),
            paired_at,
            address,
        }
    }

    /// A short, stable id for the UI: the start of the public key.
    pub fn id(&self) -> String {
        self.public_key.chars().take(16).collect()
    }

    pub(crate) fn key(&self) -> Option<[u8; 32]> {
        from_hex(&self.public_key)?.try_into().ok()
    }
}

/// Names come from the other device: one line, printable, short.
pub(crate) fn clean_name(name: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control()).take(64).collect();
    let name = name.trim();
    if name.is_empty() {
        "unnamed device".to_string()
    } else {
        name.to_string()
    }
}

pub(crate) fn load_peers(path: &Path) -> io::Result<Vec<Peer>> {
    match fs::read_to_string(path) {
        Ok(text) => {
            serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// Replaces the file atomically, so a crash never leaves half a list.
pub(crate) fn save_peers(path: &Path, peers: &[Peer]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(peers).map_err(io::Error::other)?;
    let partial = path.with_extension("json.partial");
    fs::write(&partial, text)?;
    fs::rename(&partial, path)
}
