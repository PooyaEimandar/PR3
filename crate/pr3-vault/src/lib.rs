//! PR3's vault (docs Sec. 4.1, Sec. 7.2 items 2 and 10): it holds the device's approval key
//! and signs the receipts the user approves. It hands out signatures and the public
//! key, never the private key.
//!
//! On Apple devices the key lives in the Secure Enclave and signs only after Face ID,
//! Touch ID or the device passcode ([`open_platform`]). On Android the app reaches the
//! Keystore through its own plugin, because the biometric prompt needs the app's
//! activity; it implements [`Vault`] too. [`SoftwareVault`] keeps a key in a file in
//! the app's private, backup-excluded folder: for tests, for platforms without key
//! storage, and behind a LocalAuthentication check on Apple devices without a Secure
//! Enclave (the iOS simulator).

#![deny(unsafe_code)]

#[cfg(target_vendor = "apple")]
mod apple;

use std::fmt::{self, Write as _};
use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::Path;

use p256::ecdsa::signature::Signer as _;
use p256::ecdsa::{Signature, SigningKey};
use pr3_core::trusted::{Receipt, SignedReceipt};

pub trait Vault: Send + Sync {
    /// The approval key's public half as an uncompressed SEC1 point, which the trusted
    /// side registers as the device's approval key.
    fn public_key(&self) -> Vec<u8>;

    /// Signs a receipt the user has just approved on the trusted confirmation screen.
    /// Platform vaults ask for biometrics or the passcode first, showing `reason` (for
    /// example "send the email “Lunch” to Alex Rivera"), and block until answered.
    fn sign(&self, receipt: Receipt, reason: &str) -> Result<SignedReceipt, VaultError>;

    /// Where the key lives, for logs.
    fn label(&self) -> &'static str;
}

#[derive(Debug)]
pub enum VaultError {
    Io(io::Error),
    /// The key file exists but does not hold a key.
    Corrupt,
    /// The system's random number generator failed.
    Entropy,
    /// The user dismissed the biometric or passcode prompt.
    Cancelled,
    /// The platform's key storage or authentication failed.
    Platform(String),
}

impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "vault: {e}"),
            Self::Corrupt => f.write_str("vault: the key file is damaged"),
            Self::Entropy => f.write_str("vault: no randomness available"),
            Self::Cancelled => f.write_str("the approval was cancelled"),
            Self::Platform(why) => write!(f, "vault: {why}"),
        }
    }
}

impl std::error::Error for VaultError {}

impl From<io::Error> for VaultError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// This device's vault, with its files in `dir`: the Secure Enclave on Apple devices
/// that have one, otherwise a [`SoftwareVault`] — behind Face ID, Touch ID or the
/// passcode on Apple devices. Android's Keystore vault lives in the app (see the crate
/// documentation).
pub fn open_platform(dir: &Path) -> Result<Box<dyn Vault>, VaultError> {
    #[cfg(target_vendor = "apple")]
    {
        if apple::has_secure_enclave() {
            let vault = apple::SecureEnclaveVault::open(&dir.join("vault.enclave"))?;
            // A software key from earlier builds is no longer needed.
            match fs::remove_file(dir.join(SOFTWARE_KEY_FILE)) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
            return Ok(Box::new(vault));
        }
        let key = SoftwareVault::open(&dir.join(SOFTWARE_KEY_FILE))?;
        Ok(Box::new(apple::GuardedVault::new(key)))
    }
    #[cfg(not(target_vendor = "apple"))]
    Ok(Box::new(SoftwareVault::open(&dir.join(SOFTWARE_KEY_FILE))?))
}

/// Where [`open_platform`] keeps a software key.
pub const SOFTWARE_KEY_FILE: &str = "vault.key";

/// A P-256 key in a file readable only by the app.
pub struct SoftwareVault {
    key: SigningKey,
}

impl SoftwareVault {
    /// Opens the key at `path`, creating it the first time.
    pub fn open(path: &Path) -> Result<Self, VaultError> {
        match fs::read_to_string(path) {
            Ok(text) => {
                let bytes = from_hex(text.trim()).ok_or(VaultError::Corrupt)?;
                let key = SigningKey::from_slice(&bytes).map_err(|_| VaultError::Corrupt)?;
                Ok(Self { key })
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let vault = Self::generate()?;
                vault.save(path)?;
                Ok(vault)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// A key that lives only in memory, for tests.
    pub fn generate() -> Result<Self, VaultError> {
        // A random 32-byte string is a valid P-256 key except with negligible
        // probability; try again in that case.
        for _ in 0..8 {
            let mut secret = [0u8; 32];
            getrandom::fill(&mut secret).map_err(|_| VaultError::Entropy)?;
            if let Ok(key) = SigningKey::from_slice(&secret) {
                return Ok(Self { key });
            }
        }
        Err(VaultError::Entropy)
    }

    fn save(&self, path: &Path) -> Result<(), VaultError> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut options = OpenOptions::new();
        // create_new: never overwrite a key that appeared meanwhile.
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(path)?;
        file.write_all(to_hex(&self.key.to_bytes()).as_bytes())?;
        file.sync_all()?;
        Ok(())
    }
}

impl Vault for SoftwareVault {
    fn public_key(&self) -> Vec<u8> {
        self.key
            .verifying_key()
            .to_sec1_point(false)
            .as_bytes()
            .to_vec()
    }

    fn sign(&self, receipt: Receipt, _reason: &str) -> Result<SignedReceipt, VaultError> {
        let signature: Signature = self.key.sign(&receipt.signing_bytes());
        Ok(SignedReceipt {
            receipt,
            signature: to_hex(signature.to_der().as_bytes()),
        })
    }

    fn label(&self) -> &'static str {
        "software key (dev tier)"
    }
}

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(text, "{b:02x}");
    }
    text
}

pub(crate) fn from_hex(text: &str) -> Option<Vec<u8>> {
    let (pairs, rest) = text.as_bytes().as_chunks::<2>();
    if !rest.is_empty() {
        return None;
    }
    pairs
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pr3_core::Timestamp;
    use pr3_core::trusted::{ApprovalKey, Hash, Object};

    fn receipt() -> Receipt {
        Receipt {
            request: "r1".to_string(),
            principal: "user:local".to_string(),
            actor: "agent:pr3".to_string(),
            operation: "mail.send".to_string(),
            provider: "mail:sandbox".to_string(),
            object: Object::Mail {
                to: vec!["alex@example.com".to_string()],
                subject: Hash::of(&[b"s"]),
                body: Hash::of(&[b"b"]),
            },
            approved_at: Timestamp(1),
            expires_at: Timestamp(2),
        }
    }

    #[test]
    fn signatures_verify_with_the_published_key() {
        let vault = SoftwareVault::generate().unwrap();
        let approver = ApprovalKey::from_sec1(&vault.public_key()).unwrap();
        let signed = vault.sign(receipt(), "test").unwrap();
        assert!(approver.verifies(&signed));
        let other = SoftwareVault::generate().unwrap();
        assert!(
            !ApprovalKey::from_sec1(&other.public_key())
                .unwrap()
                .verifies(&signed)
        );
    }

    #[test]
    fn the_key_file_is_created_once_private_and_reused() {
        let dir = std::env::temp_dir().join(format!("pr3-vault-test-{}", std::process::id()));
        let path = dir.join("vault.key");
        let _ = fs::remove_dir_all(&dir);
        let first = SoftwareVault::open(&path).unwrap();
        let again = SoftwareVault::open(&path).unwrap();
        assert_eq!(first.public_key(), again.public_key());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        fs::write(&path, "not a key").unwrap();
        assert!(matches!(
            SoftwareVault::open(&path),
            Err(VaultError::Corrupt)
        ));
        let _ = fs::remove_dir_all(dir);
    }
}
