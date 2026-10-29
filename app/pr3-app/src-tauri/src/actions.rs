//! Consequential actions: the host side of the trusted path (docs Sec. 4.6). A call the
//! core proposes goes to the gate in `pr3_core::trusted`; the user answers on the
//! confirmation card; the vault signs the receipt after the system's biometric or
//! passcode prompt; the gate exchanges it for a one-use capability; the sandbox outbox
//! delivers once; and every step is appended to `audit.jsonl`.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use pr3_core::trusted::{
    ApprovalKey, ApprovalRequest, Contact, Directory, Entry, Envelope, Gate, Intent, Limits,
    MAIL_SEND, Proposal, Submitted,
};
use pr3_core::{Timestamp, ToolCall, ToolSpec};
use pr3_tool::mail::{self, SandboxOutbox};
use pr3_vault::{Vault, VaultError};

/// Who proposes and on whose behalf, from the session's binding (Sec. 4.6). One user and
/// one agent until pairing (spike step 6) brings device identities.
const ACTOR: &str = "agent:pr3";
const PRINCIPAL: &str = "user:local";

pub struct Actions {
    gate: Mutex<Gate>,
    vault: Box<dyn Vault>,
    outbox: SandboxOutbox,
    directory: Directory,
    audit: PathBuf,
    /// Requests the confirmation card is showing, with where the user's answer goes.
    answers: Mutex<BTreeMap<String, mpsc::Sender<bool>>>,
}

impl Actions {
    /// Opens the outbox and audit log in `dir`, with `vault` as the approver. Refuses to
    /// start if the audit log does not verify: actions stay off until someone looks at it.
    /// Requests the app left open when it last closed are logged as expired.
    pub fn with_vault(
        dir: &Path,
        limits: Limits,
        directory: Directory,
        vault: Box<dyn Vault>,
    ) -> Result<Self, String> {
        let audit = dir.join("audit.jsonl");
        let log = read_log(&audit).map_err(|e| format!("{}: {e}", audit.display()))?;
        let approver = ApprovalKey::from_sec1(&vault.public_key())
            .ok_or("the vault's key is not a P-256 key")?;
        let mut gate =
            Gate::new(limits, approver, mail::PROVIDER, &log, now()).map_err(|broken| {
                format!(
                    "{} does not verify at entry {}; actions are off until it is checked",
                    audit.display(),
                    broken.at_index
                )
            })?;
        let closed = gate.take_log();
        append_log(&audit, &closed).map_err(|e| format!("{}: {e}", audit.display()))?;
        eprintln!(
            "PR3: actions on: {} audit entries verified, {} left open closed, approvals \
             signed by a {}",
            log.len(),
            closed.len(),
            vault.label()
        );
        Ok(Self {
            gate: Mutex::new(gate),
            vault,
            outbox: SandboxOutbox::new(dir.join("outbox.jsonl")),
            directory,
            audit,
            answers: Mutex::new(BTreeMap::new()),
        })
    }

    /// The consequential tools the model is offered.
    pub fn tools(&self) -> Vec<ToolSpec> {
        vec![mail::send_email_spec(&self.directory.contacts)]
    }

    /// Takes a call the core proposed through the trusted path; returns what the core
    /// should tell the model. `ask` shows the confirmation card; the answer arrives
    /// through [`Actions::answer`].
    pub async fn run(
        self: Arc<Self>,
        call: &ToolCall,
        ask: impl FnOnce(&ApprovalRequest),
    ) -> (bool, String) {
        let operation = match call.name.as_str() {
            mail::SEND_EMAIL => MAIL_SEND,
            other => return (false, format!("'{other}' is not a known action")),
        };
        let nonce = match nonce() {
            Ok(nonce) => nonce,
            Err(e) => return (false, e),
        };
        let envelope = Envelope {
            actor: ACTOR.to_string(),
            principal: PRINCIPAL.to_string(),
            nonce,
            received_at: now(),
            proposal: Proposal {
                operation: operation.to_string(),
                arguments: call.arguments.clone(),
            },
        };
        let submitted = self.with_gate(|gate| gate.submit(&envelope, &self.directory, now()));
        let request = match submitted {
            Some(Submitted::NeedsApproval(request)) => request,
            Some(Submitted::Refused(why)) => return (false, format!("not sent: {why}")),
            Some(Submitted::Denied(why)) => return (false, format!("not allowed: {why}")),
            None => return (false, "the trusted side is unavailable".to_string()),
        };

        let (sender, answer) = mpsc::channel();
        if let Ok(mut answers) = self.answers.lock() {
            answers.insert(request.request.clone(), sender);
        }
        ask(&request);
        let wait = Duration::from_millis(request.expires_at.0.saturating_sub(now().0));
        let approved = tauri::async_runtime::spawn_blocking(move || answer.recv_timeout(wait).ok())
            .await
            .ok()
            .flatten();
        if let Ok(mut answers) = self.answers.lock() {
            answers.remove(&request.request);
        }

        // Plain about who decided what: small models otherwise read "declined" as the
        // recipient's answer and report the email as sent.
        let id = &request.request;
        let recipients = names(&request.mail.to);
        match approved {
            Some(true) => {
                // The vault blocks on the system's prompt, so it runs off the async
                // threads.
                let reason = prompt_reason(&request);
                let this = Arc::clone(&self);
                let request = id.clone();
                tauri::async_runtime::spawn_blocking(move || this.execute(&request, &reason))
                    .await
                    .unwrap_or_else(|_| (false, "NOT SENT: the approval failed".to_string()))
            }
            Some(false) => {
                self.with_gate(|gate| gate.decline(id, "the user declined", now()));
                let said = format!(
                    "NOT SENT. The person you are talking to tapped Decline on the \
                     confirmation screen, so no email went to {recipients}."
                );
                (false, said)
            }
            None => {
                self.with_gate(|gate| gate.decline(id, "no answer before it expired", now()));
                let said = format!(
                    "NOT SENT. The person you are talking to did not approve it in time, so \
                     no email went to {recipients}."
                );
                (false, said)
            }
        }
    }

    /// The user's answer from the confirmation card.
    pub fn answer(&self, request: &str, approve: bool) -> Result<(), String> {
        let sender = self
            .answers
            .lock()
            .ok()
            .and_then(|mut answers| answers.remove(request))
            .ok_or("this request is no longer waiting")?;
        sender
            .send(approve)
            .map_err(|_| "this request is no longer waiting".to_string())
    }

    /// Approved on the card: receipt → the system's prompt and the vault's signature →
    /// capability → one delivery → audit.
    fn execute(&self, request: &str, reason: &str) -> (bool, String) {
        let Some(receipt) = self.with_gate(|gate| gate.approve(request, now())) else {
            return (false, "the trusted side is unavailable".to_string());
        };
        let receipt = match receipt {
            Ok(receipt) => receipt,
            Err(e) => return (false, format!("not sent: {e}")),
        };
        // Signed with the gate unlocked: the vault waits for Face ID, Touch ID, a
        // fingerprint or the passcode here.
        let signed = match self.vault.sign(receipt, reason) {
            Ok(signed) => signed,
            Err(VaultError::Cancelled) => {
                self.with_gate(|gate| gate.decline(request, "the user cancelled the check", now()));
                return (
                    false,
                    "NOT SENT. The person you are talking to cancelled the Face ID, \
                     fingerprint or passcode check, so the email was not sent."
                        .to_string(),
                );
            }
            Err(e) => {
                self.with_gate(|gate| gate.decline(request, &e.to_string(), now()));
                return (false, format!("NOT SENT: {e}"));
            }
        };
        let issued = self.with_gate(|gate| {
            let capability = gate.issue(&signed, now())?;
            let intent = gate.redeem(&capability, now())?;
            Ok::<_, pr3_core::trusted::GateError>((capability, intent))
        });
        let (capability, Intent::SendMail(mail)) = match issued {
            Some(Ok(issued)) => issued,
            Some(Err(e)) => return (false, format!("not sent: {e}")),
            None => return (false, "the trusted side is unavailable".to_string()),
        };
        let delivered = self.outbox.deliver(&capability, &mail, now());
        let reported = delivered.as_deref().map_err(String::as_str);
        self.with_gate(|gate| gate.executed(request, reported, now()));
        let recipients = names(&mail.to);
        match delivered {
            Ok(_) => (
                true,
                format!(
                    "SENT to {recipients}: the person you are talking to approved it on the \
                     confirmation screen (sandbox outbox)."
                ),
            ),
            Err(e) => (false, format!("NOT SENT: {e}")),
        }
    }

    /// Runs `f` on the gate and appends the audit entries it produced, still holding
    /// the lock so entries reach the file in order. `None` if the lock is poisoned.
    fn with_gate<T>(&self, f: impl FnOnce(&mut Gate) -> T) -> Option<T> {
        let mut gate = self.gate.lock().ok()?;
        let result = f(&mut gate);
        // A failed write leaves a gap that fails verification at the next start, which
        // turns actions off: losing audit entries is never silent.
        if let Err(e) = append_log(&self.audit, &gate.take_log()) {
            eprintln!("PR3: could not write {}: {e}", self.audit.display());
        }
        Some(result)
    }
}

/// What the system's prompt says is being approved: shown by the OS, outside the
/// webview, from the validated intent.
fn prompt_reason(request: &ApprovalRequest) -> String {
    let subject: String = request.mail.subject.chars().take(60).collect();
    format!("send the email “{subject}” to {}", names(&request.mail.to))
}

pub(crate) fn names(contacts: &[Contact]) -> String {
    let names: Vec<&str> = contacts.iter().map(|c| c.name.as_str()).collect();
    names.join(", ")
}

fn read_log(path: &Path) -> io::Result<Vec<Entry>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    BufReader::new(file)
        .lines()
        .map(|line| {
            serde_json::from_str(&line?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        })
        .collect()
}

fn append_log(path: &Path, entries: &[Entry]) -> io::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    for entry in entries {
        let line = serde_json::to_string(entry)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        writeln!(file, "{line}")?;
    }
    file.sync_data()
}

/// 128 bits from the system's random number generator, as hex: the envelope's nonce.
fn nonce() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| format!("no randomness available: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn now() -> Timestamp {
    crate::host::now()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pr3_core::trusted::{Receipt, SignedReceipt, verify};
    use pr3_vault::SoftwareVault;

    fn directory() -> Directory {
        Directory {
            contacts: vec![Contact {
                name: "Alex Rivera".to_string(),
                address: "alex@example.com".to_string(),
            }],
        }
    }

    fn actions(dir: &Path, limits: Limits) -> Arc<Actions> {
        let vault = SoftwareVault::generate().unwrap();
        Arc::new(Actions::with_vault(dir, limits, directory(), Box::new(vault)).unwrap())
    }

    fn call(arguments: &str) -> ToolCall {
        ToolCall {
            id: "call-1".to_string(),
            name: mail::SEND_EMAIL.to_string(),
            arguments: arguments.to_string(),
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pr3-actions-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    const TO_ALEX: &str = r#"{"body":"Noon?","subject":"Lunch","to":"Alex"}"#;

    /// Runs a call, answering the confirmation card with `answer` (or not at all).
    fn run(actions: &Arc<Actions>, arguments: &str, answer: Option<bool>) -> (bool, String) {
        tauri::async_runtime::block_on(Arc::clone(actions).run(&call(arguments), |request| {
            if let Some(approve) = answer {
                actions.answer(&request.request, approve).unwrap();
            }
        }))
    }

    #[test]
    fn a_request_left_open_by_closing_is_logged_as_expired() {
        let dir = temp("left-open");
        let first = actions(&dir, Limits::default());
        let envelope = Envelope {
            actor: ACTOR.to_string(),
            principal: PRINCIPAL.to_string(),
            nonce: nonce().unwrap(),
            received_at: now(),
            proposal: Proposal {
                operation: MAIL_SEND.to_string(),
                arguments: TO_ALEX.to_string(),
            },
        };
        // The card is showing when the app closes.
        let submitted = first.with_gate(|gate| gate.submit(&envelope, &first.directory, now()));
        let Some(Submitted::NeedsApproval(request)) = submitted else {
            panic!("expected a confirmation card");
        };
        drop(first);

        let _again = actions(&dir, Limits::default());
        let log = read_log(&dir.join("audit.jsonl")).unwrap();
        assert_eq!(verify(&log), Ok(()));
        assert_eq!(
            log.last().unwrap().event,
            pr3_core::trusted::Event::Expired {
                request: request.request
            }
        );
        // Closed once: opening again adds nothing.
        let _third = actions(&dir, Limits::default());
        assert_eq!(read_log(&dir.join("audit.jsonl")).unwrap().len(), log.len());
    }

    #[test]
    fn approved_mail_is_delivered_once_and_audited() {
        let dir = temp("approve");
        let actions = actions(&dir, Limits::default());
        let (ok, said) = run(&actions, TO_ALEX, Some(true));
        assert!(ok, "{said}");
        assert!(said.contains("Alex Rivera"));
        let outbox = fs::read_to_string(dir.join("outbox.jsonl")).unwrap();
        assert_eq!(outbox.lines().count(), 1);
        assert!(outbox.contains("alex@example.com"));

        let log = read_log(&dir.join("audit.jsonl")).unwrap();
        assert_eq!(verify(&log), Ok(()));
        assert_eq!(log.len(), 6);
        // The log proves what was sent without keeping the text.
        let text = fs::read_to_string(dir.join("audit.jsonl")).unwrap();
        assert!(!text.contains("Noon?"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn declined_unanswered_and_refused_mail_is_not_sent() {
        let dir = temp("decline");
        let quick = Limits {
            approval_ms: 200,
            ..Limits::default()
        };
        let actions = actions(&dir, quick);
        let (ok, said) = run(&actions, TO_ALEX, Some(false));
        assert!(
            !ok && said.contains("Decline") && said.contains("Alex Rivera"),
            "{said}"
        );
        let (ok, said) = run(&actions, TO_ALEX, None);
        assert!(!ok && said.contains("in time"), "{said}");
        let (ok, said) = run(
            &actions,
            r#"{"body":"b","subject":"s","to":"eve@evil.example"}"#,
            Some(true),
        );
        assert!(!ok && said.contains("address"), "{said}");
        assert!(!dir.join("outbox.jsonl").exists());
        assert!(actions.answer("no-such-request", true).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    /// A vault whose prompt the user dismisses; it records what the prompt showed.
    struct DismissedPrompt {
        key: SoftwareVault,
        shown: Arc<Mutex<Vec<String>>>,
    }

    impl Vault for DismissedPrompt {
        fn public_key(&self) -> Vec<u8> {
            self.key.public_key()
        }
        fn sign(&self, _: Receipt, reason: &str) -> Result<SignedReceipt, VaultError> {
            self.shown.lock().unwrap().push(reason.to_string());
            Err(VaultError::Cancelled)
        }
        fn label(&self) -> &'static str {
            "test vault"
        }
    }

    #[test]
    fn cancelling_the_system_prompt_sends_nothing() {
        let dir = temp("cancel");
        let shown = Arc::new(Mutex::new(Vec::new()));
        let vault = DismissedPrompt {
            key: SoftwareVault::generate().unwrap(),
            shown: Arc::clone(&shown),
        };
        let actions = Arc::new(
            Actions::with_vault(&dir, Limits::default(), directory(), Box::new(vault)).unwrap(),
        );
        let (ok, said) = run(&actions, TO_ALEX, Some(true));
        assert!(!ok && said.contains("cancelled"), "{said}");
        assert!(!dir.join("outbox.jsonl").exists());
        // The system's prompt names what is being approved.
        assert_eq!(
            shown.lock().unwrap().as_slice(),
            ["send the email “Lunch” to Alex Rivera"]
        );
        let log = read_log(&dir.join("audit.jsonl")).unwrap();
        assert!(matches!(
            log.last().map(|e| &e.event),
            Some(pr3_core::trusted::Event::Declined { .. })
        ));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_tampered_audit_log_turns_actions_off() {
        let dir = temp("tamper");
        let first = actions(&dir, Limits::default());
        run(&first, TO_ALEX, Some(true));
        drop(first);
        let path = dir.join("audit.jsonl");
        let text = fs::read_to_string(&path).unwrap();
        fs::write(
            &path,
            text.replacen("alex@example.com", "eve@evil.example", 1),
        )
        .unwrap();
        let reopened = Actions::with_vault(
            &dir,
            Limits::default(),
            Directory::default(),
            Box::new(SoftwareVault::generate().unwrap()),
        );
        assert!(reopened.err().unwrap().contains("does not verify"));
        let _ = fs::remove_dir_all(dir);
    }
}
