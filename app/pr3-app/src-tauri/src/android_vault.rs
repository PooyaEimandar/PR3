//! Android's vault: the approval key in the Keystore, through `VaultPlugin.kt` in the
//! app's Android project. It lives in the app rather than in `pr3-vault` because
//! BiometricPrompt needs the app's activity.

use pr3_core::trusted::{Receipt, SignedReceipt};
use pr3_vault::{Vault, VaultError};
use serde::Deserialize;
use serde_json::json;
use tauri::plugin::mobile::PluginInvokeError;
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{Manager, Wry};

/// The Kotlin plugin, registered at startup.
pub struct VaultHandle(pub PluginHandle<Wry>);

pub fn plugin() -> TauriPlugin<Wry> {
    Builder::new("pr3-vault")
        .setup(|app, api| {
            let handle = api.register_android_plugin("ai.pooya.pr3", "VaultPlugin")?;
            app.manage(VaultHandle(handle));
            Ok(())
        })
        .build()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicKeyReply {
    public_key: String,
    storage: String,
}

#[derive(Deserialize)]
struct SignReply {
    signature: String,
}

pub struct KeystoreVault {
    handle: PluginHandle<Wry>,
    public: Vec<u8>,
    label: &'static str,
}

impl KeystoreVault {
    /// Creates the key the first time. Fails if the phone has no secure screen lock:
    /// without one there is nothing to approve with.
    pub fn open(handle: PluginHandle<Wry>) -> Result<Self, VaultError> {
        let reply: PublicKeyReply = handle
            .run_mobile_plugin("publicKey", ())
            .map_err(platform)?;
        let public = from_hex(&reply.public_key).ok_or(VaultError::Corrupt)?;
        let label = match reply.storage.as_str() {
            "StrongBox" => {
                "Android Keystore key in StrongBox, approved with biometrics or the screen lock"
            }
            "TEE" => "Android Keystore key in the TEE, approved with biometrics or the screen lock",
            _ => {
                "Android Keystore key (software-backed, as on emulators), approved with biometrics or the screen lock"
            }
        };
        Ok(Self {
            handle,
            public,
            label,
        })
    }
}

impl Vault for KeystoreVault {
    fn public_key(&self) -> Vec<u8> {
        self.public.clone()
    }

    fn sign(&self, receipt: Receipt, reason: &str) -> Result<SignedReceipt, VaultError> {
        let message: String = receipt
            .signing_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let reply: SignReply = self
            .handle
            .run_mobile_plugin("sign", json!({ "message": message, "reason": reason }))
            .map_err(platform)?;
        Ok(SignedReceipt {
            receipt,
            signature: reply.signature,
        })
    }

    fn label(&self) -> &'static str {
        self.label
    }
}

fn platform(error: PluginInvokeError) -> VaultError {
    match error {
        PluginInvokeError::InvokeRejected(e) if e.code.as_deref() == Some("cancelled") => {
            VaultError::Cancelled
        }
        PluginInvokeError::InvokeRejected(e) => VaultError::Platform(
            e.message
                .unwrap_or_else(|| "the Keystore failed".to_string()),
        ),
        other => VaultError::Platform(other.to_string()),
    }
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    let (pairs, rest) = text.as_bytes().as_chunks::<2>();
    if !rest.is_empty() {
        return None;
    }
    pairs
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect()
}
