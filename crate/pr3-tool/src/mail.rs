//! The sandbox mail connector (docs Sec. 4.7, M5-E1), the spike's one consequential
//! action. Delivering appends the message to an outbox file in the app's data folder;
//! nothing leaves the device, so the whole trusted path can be exercised safely.
//!
//! The model is offered `send_email`, which the core never runs directly
//! (`requires_confirmation`): the host takes each call through the trusted side
//! (`pr3_core::trusted`) and calls [`SandboxOutbox::deliver`] only with the capability
//! the user's approval produced.

use std::fs::{self, OpenOptions};
use std::io::{self, BufRead as _, BufReader, Write as _};
use std::path::PathBuf;
use std::sync::Mutex;

use pr3_core::trusted::{Capability, Contact, Intent, MAIL_SEND, Mail, Object};
use pr3_core::{Timestamp, ToolSpec};
use serde_json::json;

pub const SEND_EMAIL: &str = "send_email";
/// The provider capabilities name for this connector.
pub const PROVIDER: &str = "mail:sandbox";

/// The tool offered to the model. It names the user's contacts so the model can refer
/// to them; addresses stay with the host.
pub fn send_email_spec(contacts: &[Contact]) -> ToolSpec {
    let names: Vec<&str> = contacts.iter().map(|c| c.name.as_str()).collect();
    let who = if names.is_empty() {
        "The user has no contacts yet, so nothing can be sent.".to_string()
    } else {
        format!("The user's contacts: {}.", names.join(", "))
    };
    ToolSpec {
        name: SEND_EMAIL.to_string(),
        description: format!(
            "Send an email in the user's name. {who} PR3 shows the user the exact email \
             and sends it only if they approve; if they decline, nothing is sent."
        ),
        parameters: r#"{"type":"object","properties":{"to":{"type":"string","description":"Contact names, separated by commas"},"subject":{"type":"string","description":"One line"},"body":{"type":"string","description":"The full text of the email"}},"required":["to","subject","body"]}"#.to_string(),
        requires_confirmation: true,
        required_capabilities: vec![MAIL_SEND.to_string()],
    }
}

/// The sandbox mail provider: an append-only JSON Lines outbox.
pub struct SandboxOutbox {
    path: PathBuf,
    lock: Mutex<()>,
}

impl SandboxOutbox {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    /// Delivers `mail` under `capability`, whose id is the idempotency key: delivering
    /// under the same capability again reports the first delivery instead of sending
    /// twice (docs Sec. 7.2 item 6). The message must be exactly the one approved.
    pub fn deliver(
        &self,
        capability: &Capability,
        mail: &Mail,
        at: Timestamp,
    ) -> Result<String, String> {
        if capability.operation != MAIL_SEND || capability.provider != PROVIDER {
            return Err(format!(
                "this capability is for {} on {}",
                capability.operation, capability.provider
            ));
        }
        if Object::of(&Intent::SendMail(mail.clone())) != capability.object {
            return Err("the message is not the one the user approved".to_string());
        }
        let _guard = self.lock.lock().map_err(|_| "the outbox is unavailable")?;
        if self.delivered(&capability.id).map_err(|e| e.to_string())? {
            return Ok(format!("already delivered as {}", capability.id));
        }
        let record = json!({
            "id": capability.id,
            "at": at.0,
            "to": mail.to.iter().map(|c| format!("{} <{}>", c.name, c.address)).collect::<Vec<_>>(),
            "subject": mail.subject,
            "body": mail.body,
            "receipt": capability.receipt.to_hex(),
        });
        self.append(&record.to_string())
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "delivered to the sandbox outbox as {}",
            capability.id
        ))
    }

    fn delivered(&self, id: &str) -> io::Result<bool> {
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e),
        };
        for line in BufReader::new(file).lines() {
            let line = line?;
            let record: serde_json::Value = serde_json::from_str(&line).unwrap_or_default();
            if record.get("id").and_then(serde_json::Value::as_str) == Some(id) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn append(&self, line: &str) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{line}")?;
        file.sync_data()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pr3_core::trusted::Hash;

    fn mail() -> Mail {
        Mail {
            to: vec![Contact {
                name: "Alex Rivera".to_string(),
                address: "alex@example.com".to_string(),
            }],
            subject: "Lunch".to_string(),
            body: "Noon?".to_string(),
        }
    }

    fn capability(id: &str, mail: &Mail) -> Capability {
        Capability {
            id: id.to_string(),
            principal: "user:local".to_string(),
            actor: "agent:pr3".to_string(),
            operation: MAIL_SEND.to_string(),
            provider: PROVIDER.to_string(),
            object: Object::of(&Intent::SendMail(mail.clone())),
            receipt: Hash::of(&[b"receipt"]),
            issued_at: Timestamp(1),
            expires_at: Timestamp(2),
            max_uses: 1,
        }
    }

    #[test]
    fn delivers_once_per_capability_and_only_the_approved_message() {
        let dir = std::env::temp_dir().join(format!("pr3-outbox-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let outbox = SandboxOutbox::new(dir.join("outbox.jsonl"));
        let mail = mail();
        let cap = capability("r1", &mail);

        assert!(
            outbox
                .deliver(&cap, &mail, Timestamp(5))
                .unwrap()
                .contains("delivered")
        );
        assert!(
            outbox
                .deliver(&cap, &mail, Timestamp(6))
                .unwrap()
                .contains("already")
        );
        let lines = fs::read_to_string(dir.join("outbox.jsonl")).unwrap();
        assert_eq!(lines.lines().count(), 1);

        let mut changed = mail.clone();
        changed.body.push_str(" Bring the files.");
        assert!(
            outbox
                .deliver(&capability("r2", &mail), &changed, Timestamp(7))
                .is_err()
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_spec_is_accepted_by_the_core_and_needs_confirmation() {
        let spec = send_email_spec(&mail().to);
        assert!(spec.requires_confirmation);
        assert!(spec.description.contains("Alex Rivera"));
        let mut core = pr3_core::Core::new(pr3_core::Config::default());
        core.register_tool(spec).unwrap();
    }
}
