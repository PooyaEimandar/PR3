//! Apple platforms, through the Swift half in `swift/`: the approval key lives in the
//! Secure Enclave and signs only after Face ID, Touch ID or the device passcode. Without
//! a Secure Enclave, a software key signs after the same check.
//!
//! This is the crate's one audited `unsafe` boundary (docs Sec. 4.2): plain C calls into
//! Swift, with strings in and JSON strings out.

#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char};
use std::path::Path;

use pr3_core::trusted::{Receipt, SignedReceipt};
use serde_json::Value;

use crate::{SoftwareVault, Vault, VaultError, from_hex};

unsafe extern "C" {
    fn pr3_vault_free(text: *mut c_char);
    fn pr3_vault_has_secure_enclave() -> bool;
    fn pr3_vault_enclave_public_key(path: *const c_char) -> *mut c_char;
    fn pr3_vault_enclave_sign(
        path: *const c_char,
        message: *const u8,
        length: isize,
        reason: *const c_char,
    ) -> *mut c_char;
    fn pr3_vault_authenticate(reason: *const c_char) -> *mut c_char;
}

pub(crate) fn has_secure_enclave() -> bool {
    // SAFETY: takes no arguments and only reads a system property.
    unsafe { pr3_vault_has_secure_enclave() }
}

/// Takes ownership of a reply from the Swift side and frees it: the hex payload of
/// `{"ok": …}`, or the error it reports.
fn take(reply: *mut c_char) -> Result<String, VaultError> {
    if reply.is_null() {
        return Err(VaultError::Platform(
            "no reply from the Secure Enclave".to_string(),
        ));
    }
    // SAFETY: every Swift function returns a NUL-terminated string from strdup, which
    // is read here and then freed exactly once, by the side that allocated it.
    let text = unsafe { CStr::from_ptr(reply) }
        .to_string_lossy()
        .into_owned();
    // SAFETY: as above; `reply` is not used after this.
    unsafe { pr3_vault_free(reply) };
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| VaultError::Platform("unreadable reply from the Swift side".to_string()))?;
    if let Some(ok) = value.get("ok").and_then(Value::as_str) {
        return Ok(ok.to_string());
    }
    if value.get("cancelled") == Some(&Value::Bool(true)) {
        return Err(VaultError::Cancelled);
    }
    let why = value
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("unknown error");
    Err(VaultError::Platform(why.to_string()))
}

fn c_string(text: &str) -> CString {
    // Interior NULs cannot occur in paths or prompts PR3 builds; drop any, never fail.
    CString::new(text.replace('\0', "")).unwrap_or_default()
}

/// The approval key in the Secure Enclave. `wrapped` holds the key in the form only
/// this device's Secure Enclave can use.
pub(crate) struct SecureEnclaveVault {
    wrapped: CString,
    public: Vec<u8>,
}

impl SecureEnclaveVault {
    pub(crate) fn open(wrapped: &Path) -> Result<Self, VaultError> {
        let wrapped = c_string(&wrapped.to_string_lossy());
        // SAFETY: `wrapped` is a valid C string for the duration of the call.
        let reply = unsafe { pr3_vault_enclave_public_key(wrapped.as_ptr()) };
        let public = from_hex(&take(reply)?).ok_or(VaultError::Corrupt)?;
        Ok(Self { wrapped, public })
    }
}

impl Vault for SecureEnclaveVault {
    fn public_key(&self) -> Vec<u8> {
        self.public.clone()
    }

    fn sign(&self, receipt: Receipt, reason: &str) -> Result<SignedReceipt, VaultError> {
        let message = receipt.signing_bytes();
        let reason = c_string(reason);
        let length = isize::try_from(message.len()).map_err(|_| VaultError::Corrupt)?;
        // SAFETY: the strings and `message` stay alive for the call, and `length` is
        // `message`'s length; Swift copies the bytes before returning.
        let reply = unsafe {
            pr3_vault_enclave_sign(
                self.wrapped.as_ptr(),
                message.as_ptr(),
                length,
                reason.as_ptr(),
            )
        };
        Ok(SignedReceipt {
            receipt,
            signature: take(reply)?,
        })
    }

    fn label(&self) -> &'static str {
        "Secure Enclave key, approved with Face ID, Touch ID or the passcode"
    }
}

/// Without a Secure Enclave: a software key that signs only after the same check.
/// Weaker — the key is a file the check guards, not hardware — and labelled so.
pub(crate) struct GuardedVault {
    key: SoftwareVault,
}

impl GuardedVault {
    pub(crate) fn new(key: SoftwareVault) -> Self {
        Self { key }
    }
}

impl Vault for GuardedVault {
    fn public_key(&self) -> Vec<u8> {
        self.key.public_key()
    }

    fn sign(&self, receipt: Receipt, reason: &str) -> Result<SignedReceipt, VaultError> {
        let reason = c_string(reason);
        // SAFETY: `reason` is a valid C string for the duration of the call.
        take(unsafe { pr3_vault_authenticate(reason.as_ptr()) })?;
        self.key.sign(receipt, "")
    }

    fn label(&self) -> &'static str {
        "software key behind Face ID, Touch ID or the passcode (no Secure Enclave)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creating and reloading a Secure Enclave key needs no prompt; signing does, so it
    /// is tested in the app. Skipped where there is no Secure Enclave (CI's VMs).
    #[test]
    fn an_enclave_key_is_created_once_and_reloaded() {
        if !has_secure_enclave() {
            eprintln!("no Secure Enclave here; skipped");
            return;
        }
        let dir = std::env::temp_dir().join(format!("pr3-enclave-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("vault.enclave");
        let first = match SecureEnclaveVault::open(&path) {
            // errSecInteractionNotAllowed: the screen is locked, and these keys exist only
            // for an unlocked device, as intended.
            Err(VaultError::Platform(why)) if why.contains("-25308") => {
                eprintln!("the screen is locked; skipped");
                return;
            }
            other => other.unwrap(),
        };
        let again = SecureEnclaveVault::open(&path).unwrap();
        assert_eq!(first.public_key().len(), 65);
        assert_eq!(first.public_key(), again.public_key());
        assert!(pr3_core::trusted::ApprovalKey::from_sec1(&first.public_key()).is_some());

        std::fs::write(&path, b"not a key").unwrap();
        assert!(
            SecureEnclaveVault::open(&path).is_err(),
            "a damaged key is never replaced"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
