//! Consequential actions: the host side of the trusted path (docs Sec. 4.6). A call the
//! core proposes goes to the gate in `pr3_core::trusted`; the user answers on the
//! confirmation card; the vault signs the receipt after the system's biometric or
//! passcode prompt; the gate exchanges it for a one-use capability; the sandbox outbox
//! delivers once; and every step is appended to `audit.jsonl`.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use pr3_core::trusted::{
    ApprovalKey, ApprovalRequest, Contact, Directory, Entry, Envelope, Gate, Intent, Limits,
    MAIL_SEND, Mail, Proposal, Submitted,
};
use pr3_core::{Timestamp, ToolCall, ToolSpec};
use pr3_tool::mail::{self, SandboxOutbox};
use pr3_vault::{Vault, VaultError};

/// Who proposes and on whose behalf, from the session's binding (Sec. 4.6). One user and
/// one agent until pairing (spike step 6) brings device identities.
const ACTOR: &str = "agent:pr3";
const PRINCIPAL: &str = "user:local";
pub(crate) const AUDIT_FAILED: &str =
    "The action audit could not be saved. Further actions are disabled until PR3 restarts.";

struct AuditedGate {
    gate: Gate,
    /// Never retry a potentially partial append. Retain the entries for diagnosis
    /// and latch the failure until the log is verified again on restart.
    pending: Vec<Entry>,
    failure: Option<String>,
}

pub struct Actions {
    gate: Mutex<AuditedGate>,
    vault: Box<dyn Vault>,
    outbox: SandboxOutbox,
    directory: Directory,
    device: Option<DeviceMail>,
    audit: PathBuf,
    /// Requests the confirmation card is showing, with where the user's answer goes.
    answers: Mutex<BTreeMap<String, mpsc::Sender<bool>>>,
}

type OpenDraft = dyn Fn(&pr3_core::trusted::Mail) -> Result<(), String> + Send + Sync;

struct DeviceMail {
    book: Arc<crate::contacts::Book>,
    open: Box<OpenDraft>,
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
        Self::open(dir, limits, directory, vault, None)
    }

    pub fn with_device(
        dir: &Path,
        limits: Limits,
        vault: Box<dyn Vault>,
        book: Arc<crate::contacts::Book>,
        open: Box<OpenDraft>,
    ) -> Result<Self, String> {
        Self::open(
            dir,
            limits,
            Directory::default(),
            vault,
            Some(DeviceMail { book, open }),
        )
    }

    fn open(
        dir: &Path,
        limits: Limits,
        directory: Directory,
        vault: Box<dyn Vault>,
        device: Option<DeviceMail>,
    ) -> Result<Self, String> {
        let audit = dir.join("audit.jsonl");
        let log = read_log(&audit).map_err(|e| format!("{}: {e}", audit.display()))?;
        let approver = ApprovalKey::from_sec1(&vault.public_key())
            .ok_or("the vault's key is not a P-256 key")?;
        let provider = if device.is_some() {
            crate::device_mail::PROVIDER
        } else {
            mail::PROVIDER
        };
        let mut gate = Gate::new(limits, approver, provider, &log, now()).map_err(|broken| {
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
            gate: Mutex::new(AuditedGate {
                gate,
                pending: Vec::new(),
                failure: None,
            }),
            vault,
            outbox: SandboxOutbox::new(dir.join("outbox.jsonl")),
            directory,
            device,
            audit,
            answers: Mutex::new(BTreeMap::new()),
        })
    }

    /// The consequential tools the model is offered.
    pub fn tools(&self) -> Vec<ToolSpec> {
        let mut spec = mail::send_email_spec(&self.directory.contacts);
        if self.device.is_some() {
            spec.description = "Prepare an email draft for the device's email app. Name a contact from the user's request; PR3 resolves their email address from the permission-enabled device address book. Never invent an address. PR3 shows the exact recipients and text for approval before opening the email app, where the user reviews and presses Send. Opening a draft is NOT sending, delivery or confirmation that a message is queued. If contacts are missing or ambiguous, ask the user to enable contact sync in Settings → Contacts or choose the exact contact name.".into();
        }
        vec![spec]
    }

    /// Takes a call the core proposed through the trusted path; returns what the core
    /// should tell the model. `ask` shows the confirmation card; the answer arrives
    /// through [`Actions::answer`].
    pub async fn run(
        self: Arc<Self>,
        call: &ToolCall,
        ask: impl FnOnce(&ApprovalRequest),
    ) -> (bool, String) {
        self.run_controlled(call, ask, Arc::new(AtomicBool::new(false)), || Ok(()))
            .await
    }

    /// Cancellation never detaches execution. The caller receives the actual result
    /// if the effect has already started. `prepare` persists a recovery checkpoint.
    pub async fn run_controlled(
        self: Arc<Self>,
        call: &ToolCall,
        ask: impl FnOnce(&ApprovalRequest),
        cancel: Arc<AtomicBool>,
        prepare: impl FnOnce() -> Result<(), String> + Send + 'static,
    ) -> (bool, String) {
        self.run_checked(call, |_| Ok(()), ask, cancel, prepare)
            .await
    }

    /// Check task-specific content restrictions on the canonical email before any
    /// card, signing or execution. The approved object is never rewritten.
    pub(crate) async fn run_checked(
        self: Arc<Self>,
        call: &ToolCall,
        check: impl FnOnce(&Mail) -> Result<(), String>,
        ask: impl FnOnce(&ApprovalRequest),
        cancel: Arc<AtomicBool>,
        prepare: impl FnOnce() -> Result<(), String> + Send + 'static,
    ) -> (bool, String) {
        if cancel.load(Ordering::Acquire) {
            return (false, "NOT SENT: stopped by you.".to_string());
        }
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
        let directory = if let Some(device) = &self.device {
            let book = device.book.clone();
            match tauri::async_runtime::spawn_blocking(move || book.directory()).await {
                Ok(Ok(directory)) => directory,
                Ok(Err(error)) => return (false, format!("NOT SENT: {error}")),
                Err(_) => return (false, "NOT SENT: contacts are unavailable.".into()),
            }
        } else {
            self.directory.clone()
        };
        if cancel.load(Ordering::Acquire) {
            return (false, "NOT SENT: stopped by you.".into());
        }
        let submitted = self.with_gate(|gate| gate.submit(&envelope, &directory, now()));
        let request = match submitted {
            Ok(Submitted::NeedsApproval(request)) => request,
            Ok(Submitted::Refused(why)) => return (false, format!("not sent: {why}")),
            Ok(Submitted::Denied(why)) => return (false, format!("not allowed: {why}")),
            Err(error) => return (false, format!("NOT SENT: {error}")),
        };

        if let Err(reason) = check(&request.mail) {
            let _ = self.with_gate(|gate| gate.decline(&request.request, &reason, now()));
            return (false, format!("NOT SENT: {reason}"));
        }

        let (sender, answer) = mpsc::channel();
        if let Ok(mut answers) = self.answers.lock() {
            answers.insert(request.request.clone(), sender);
        }
        ask(&request);
        let until = request.expires_at;
        let stopped = Arc::clone(&cancel);
        let approved = tauri::async_runtime::spawn_blocking(move || {
            loop {
                if stopped.load(Ordering::Acquire) || now() >= until {
                    return None;
                }
                let wait = Duration::from_millis(until.0.saturating_sub(now().0).min(50));
                match answer.recv_timeout(wait) {
                    Ok(approve) => return Some(approve),
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => return None,
                }
            }
        })
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
        if cancel.load(Ordering::Acquire) {
            let _ = self.with_gate(|gate| gate.decline(id, "the user stopped the task", now()));
            return (false, "NOT SENT: stopped by you.".to_string());
        }
        match approved {
            Some(true) => {
                // The vault blocks on the system's prompt, so it runs off the async
                // threads.
                let reason = prompt_reason(&request);
                let this = Arc::clone(&self);
                let request = id.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    this.execute(&request, &reason, &cancel, prepare)
                })
                .await
                .unwrap_or_else(|_| (false, crate::task::UNKNOWN.to_string()))
            }
            Some(false) => {
                let _ = self.with_gate(|gate| gate.decline(id, "the user declined", now()));
                let said = format!(
                    "NOT SENT. The person you are talking to tapped Decline on the \
                     confirmation screen, so no email went to {recipients}."
                );
                (false, said)
            }
            None => {
                let _ =
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
    fn execute(
        &self,
        request: &str,
        reason: &str,
        cancel: &AtomicBool,
        prepare: impl FnOnce() -> Result<(), String>,
    ) -> (bool, String) {
        if cancel.load(Ordering::Acquire) {
            let _ =
                self.with_gate(|gate| gate.decline(request, "the user stopped the task", now()));
            return (false, "NOT SENT: stopped by you.".to_string());
        }
        if let Err(error) = prepare() {
            let _ = self.with_gate(|gate| {
                gate.decline(request, "could not record execution checkpoint", now())
            });
            return (false, format!("NOT SENT: {error}"));
        }
        let receipt = match self.with_gate(|gate| gate.approve(request, now())) {
            Ok(receipt) => receipt,
            Err(error) => return (false, format!("NOT SENT: {error}")),
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
                let _ = self
                    .with_gate(|gate| gate.decline(request, "the user cancelled the check", now()));
                return (
                    false,
                    "NOT SENT. The person you are talking to cancelled the Face ID, \
                     fingerprint or passcode check, so the email was not sent."
                        .to_string(),
                );
            }
            Err(e) => {
                let _ = self.with_gate(|gate| gate.decline(request, &e.to_string(), now()));
                return (false, format!("NOT SENT: {e}"));
            }
        };
        // Stop during the OS prompt must not become an execution after it closes.
        if cancel.load(Ordering::Acquire) {
            let _ =
                self.with_gate(|gate| gate.decline(request, "the user stopped the task", now()));
            return (false, "NOT SENT: stopped by you.".to_string());
        }
        let issued = self.with_gate(|gate| {
            let capability = gate.issue(&signed, now())?;
            let intent = gate.redeem(&capability, now())?;
            Ok::<_, pr3_core::trusted::GateError>((capability, intent))
        });
        let (capability, Intent::SendMail(mail)) = match issued {
            Ok(Ok(issued)) => issued,
            Ok(Err(e)) => return (false, format!("not sent: {e}")),
            Err(error) => return (false, format!("NOT SENT: {error}")),
        };
        let delivered = if let Some(device) = &self.device {
            device.book.with_recipients(&mail.to, || {
                if cancel.load(Ordering::Acquire) {
                    return Err("stopped by you".into());
                }
                (device.open)(&mail)
                    .map(|()| "draft opened in device email app; delivery unverified".into())
            })
        } else {
            self.outbox.deliver(&capability, &mail, now())
        };
        let reported = delivered.as_deref().map_err(String::as_str);
        let audit = self.with_gate(|gate| gate.executed(request, reported, now()));
        let recipients = names(&mail.to);
        let (success, mut output) = match delivered {
            Ok(_) if self.device.is_some() => (
                true,
                format!(
                    "{} for {recipients}. Review it and press Send there. Sending and delivery are not verified by PR3; an internet connection is needed to deliver it.",
                    crate::device_mail::OPENED
                ),
            ),
            Ok(_) => (
                true,
                format!(
                    "Email to {recipients} added to the sandbox outbox after approval. \
                     Nothing was sent externally."
                ),
            ),
            Err(e) if e == crate::task::UNKNOWN => (false, e),
            Err(e) => (false, format!("NOT SENT: {e}")),
        };
        // The effect has already happened: preserve the real result, and never
        // turn an audit failure into a claim that it is safe to send again.
        if audit.is_err() {
            output.push_str(&format!(" {AUDIT_FAILED}"));
        }
        (success, output)
    }

    /// Runs `f` on the gate and appends the audit entries it produced, still holding
    /// the lock so entries reach the file in order. No result authorizes execution
    /// unless its audit entries have reached durable storage.
    fn with_gate<T>(&self, f: impl FnOnce(&mut Gate) -> T) -> Result<T, String> {
        let mut state = self
            .gate
            .lock()
            .map_err(|_| "the trusted side is unavailable")?;
        if let Some(error) = &state.failure {
            return Err(error.clone());
        }
        let result = f(&mut state.gate);
        state.pending = state.gate.take_log();
        if let Err(e) = append_log(&self.audit, &state.pending) {
            eprintln!("PR3: could not write {}: {e}", self.audit.display());
            state.failure = Some(AUDIT_FAILED.into());
            return Err(AUDIT_FAILED.into());
        }
        state.pending.clear();
        Ok(result)
    }
}

/// What the system's prompt says is being approved: shown by the OS, outside the
/// webview, from the validated intent. Apple prints it after "PR3 is trying to".
fn prompt_reason(request: &ApprovalRequest) -> String {
    let reason = reason_for(&request.mail.to, &request.mail.subject);
    if request.provider == crate::device_mail::PROVIDER {
        reason.replacen("send an email to ", "open an email draft to ", 1)
    } else {
        reason
    }
}

/// The recipients come first, from the address book, so a long subject the model wrote
/// cannot push them out of a prompt that shows one line; and the subject's own quotation
/// marks become apostrophes, so it cannot close its quote and name someone else.
fn reason_for(to: &[Contact], subject: &str) -> String {
    let subject: String = subject
        .chars()
        .take(60)
        .map(|c| if quotation_mark(c) { '\'' } else { c })
        .collect();
    format!("send an email to {}: “{subject}”", names(to))
}

fn quotation_mark(c: char) -> bool {
    matches!(
        c,
        '"' | '“'
            | '”'
            | '„'
            | '‟'
            | '«'
            | '»'
            | '‹'
            | '›'
            | '＂'
            | '〝'
            | '〞'
            | '〟'
            | '❝'
            | '❞'
    )
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
    fn cancellation_during_device_check_and_failed_checkpoints_never_deliver() {
        struct StopOnSign {
            vault: SoftwareVault,
            cancel: Arc<AtomicBool>,
        }
        impl Vault for StopOnSign {
            fn public_key(&self) -> Vec<u8> {
                self.vault.public_key()
            }
            fn label(&self) -> &'static str {
                "test vault"
            }
            fn sign(
                &self,
                receipt: pr3_core::trusted::Receipt,
                reason: &str,
            ) -> Result<pr3_core::trusted::SignedReceipt, VaultError> {
                self.cancel.store(true, Ordering::Release);
                self.vault.sign(receipt, reason)
            }
        }
        for fail_checkpoint in [false, true] {
            let dir = temp(if fail_checkpoint {
                "checkpoint-failed"
            } else {
                "stop-during-sign"
            });
            let cancel = Arc::new(AtomicBool::new(false));
            let actions = Arc::new(
                Actions::with_vault(
                    &dir,
                    Limits::default(),
                    directory(),
                    Box::new(StopOnSign {
                        vault: SoftwareVault::generate().unwrap(),
                        cancel: cancel.clone(),
                    }),
                )
                .unwrap(),
            );
            let (success, result) = tauri::async_runtime::block_on(actions.clone().run_controlled(
                &call(TO_ALEX),
                |request| actions.answer(&request.request, true).unwrap(),
                cancel.clone(),
                move || {
                    if fail_checkpoint {
                        Err("storage unavailable".into())
                    } else {
                        Ok(())
                    }
                },
            ));
            assert!(!success);
            assert!(result.starts_with("NOT SENT:"));
            assert!(!dir.join("outbox.jsonl").exists());
            assert_eq!(
                cancel.load(Ordering::Acquire),
                !fail_checkpoint,
                "checkpoint failure must precede the vault"
            );
            assert!(actions.answers.lock().unwrap().is_empty());
            assert_eq!(verify(&read_log(&dir.join("audit.jsonl")).unwrap()), Ok(()));
            fs::remove_dir_all(dir).unwrap();
        }
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
        let Ok(Submitted::NeedsApproval(request)) = submitted else {
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

    fn break_audit(path: &Path) {
        if path.is_file() {
            fs::rename(path, path.with_extension("saved")).unwrap();
        }
        fs::create_dir(path).unwrap();
    }

    #[test]
    fn audit_failures_before_execution_block_delivery_and_latch_actions_off() {
        for before_card in [true, false] {
            let dir = temp(if before_card {
                "audit-submit"
            } else {
                "audit-approve"
            });
            let actions = actions(&dir, Limits::default());
            let audit = dir.join("audit.jsonl");
            if before_card {
                break_audit(&audit);
            }
            let (ok, output) =
                tauri::async_runtime::block_on(actions.clone().run(&call(TO_ALEX), |request| {
                    assert!(!before_card, "no card before its audit is committed");
                    break_audit(&audit);
                    actions.answer(&request.request, true).unwrap();
                }));
            assert!(!ok);
            assert!(output.contains(AUDIT_FAILED));
            assert!(!dir.join("outbox.jsonl").exists());
            assert!(!actions.gate.lock().unwrap().pending.is_empty());
            // A repair must not retry an append that could have partially committed.
            fs::remove_dir(&audit).unwrap();
            if audit.with_extension("saved").is_file() {
                fs::rename(audit.with_extension("saved"), &audit).unwrap();
            }
            assert!(!run(&actions, TO_ALEX, Some(true)).0);
            assert!(!dir.join("outbox.jsonl").exists());
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn audit_failure_after_signing_still_prevents_execution() {
        struct BreakOnSign {
            vault: SoftwareVault,
            audit: PathBuf,
        }
        impl Vault for BreakOnSign {
            fn public_key(&self) -> Vec<u8> {
                self.vault.public_key()
            }
            fn label(&self) -> &'static str {
                "test vault"
            }
            fn sign(
                &self,
                receipt: pr3_core::trusted::Receipt,
                reason: &str,
            ) -> Result<pr3_core::trusted::SignedReceipt, VaultError> {
                let signed = self.vault.sign(receipt, reason)?;
                break_audit(&self.audit);
                Ok(signed)
            }
        }
        let dir = temp("audit-capability");
        let actions = Arc::new(
            Actions::with_vault(
                &dir,
                Limits::default(),
                directory(),
                Box::new(BreakOnSign {
                    vault: SoftwareVault::generate().unwrap(),
                    audit: dir.join("audit.jsonl"),
                }),
            )
            .unwrap(),
        );
        let (ok, output) = run(&actions, TO_ALEX, Some(true));
        assert!(!ok && output.contains(AUDIT_FAILED));
        assert!(!dir.join("outbox.jsonl").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn audit_failure_after_opening_a_draft_preserves_the_real_result_without_retry() {
        for unknown in [false, true] {
            struct Contacts;
            impl crate::contacts::Source for Contacts {
                fn read(&self, _: bool) -> Result<crate::contacts::Snapshot, String> {
                    Ok(serde_json::from_value(
                        serde_json::json!({"permission":"granted", "contacts": [
                            {"name":"Alex Rivera", "address":"alex@example.com"}
                        ]}),
                    )
                    .unwrap())
                }
            }
            let dir = temp(if unknown {
                "audit-after-uncertain-handoff"
            } else {
                "audit-after-handoff"
            });
            fs::write(dir.join("contacts-sync"), b"enabled\n").unwrap();
            let book = Arc::new(crate::contacts::Book::open(&dir, Box::new(Contacts)));
            let opened = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let calls = opened.clone();
            let audit = dir.join("audit.jsonl");
            let actions = Arc::new(
                Actions::with_device(
                    &dir,
                    Limits::default(),
                    Box::new(SoftwareVault::generate().unwrap()),
                    book,
                    Box::new(move |_| {
                        calls.fetch_add(1, Ordering::SeqCst);
                        break_audit(&audit);
                        if unknown {
                            Err(crate::task::UNKNOWN.into())
                        } else {
                            Ok(())
                        }
                    }),
                )
                .unwrap(),
            );
            let (ok, output) = run(&actions, TO_ALEX, Some(true));
            assert_eq!(ok, !unknown, "{output}");
            if unknown {
                assert!(crate::task::unknown(&output));
            } else {
                assert!(output.starts_with(crate::device_mail::OPENED));
            }
            assert!(output.contains(AUDIT_FAILED));
            let mut tasks = crate::task::Tasks::default();
            let (id, _) = tasks.begin("Email Alex", now());
            tasks.record(
                id,
                &crate::host::TurnEvent::Tool {
                    sources: None,
                    name: "send_email".into(),
                    arguments: TO_ALEX.into(),
                    success: ok,
                    output: output.clone(),
                },
                now(),
            );
            tasks.end(id, Ok(output), now());
            let view = tasks.view(id, now()).unwrap();
            assert_eq!(view.card.page, crate::task::Page::Todo);
            assert!(!view.retryable);
            assert!(tasks.result_line(id).unwrap().contains(AUDIT_FAILED));
            assert!(!run(&actions, TO_ALEX, Some(true)).0);
            assert_eq!(opened.load(Ordering::SeqCst), 1);
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn email_exclusions_keep_approval_and_exact_handoff_for_revised_drafts() {
        struct Contacts;
        impl crate::contacts::Source for Contacts {
            fn read(&self, _: bool) -> Result<crate::contacts::Snapshot, String> {
                Ok(serde_json::from_value(serde_json::json!({"permission":"granted","contacts":[{"name":"Alex Rivera","address":"alex@example.com"}]})).unwrap())
            }
        }
        for device in [false, true] {
            let dir = temp(if device {
                "exclusion-device"
            } else {
                "exclusion-sandbox"
            });
            let handed_off = Arc::new(Mutex::new(Vec::<Mail>::new()));
            let actions = Arc::new(if device {
                fs::write(dir.join("contacts-sync"), b"enabled\n").unwrap();
                let opened = handed_off.clone();
                Actions::with_device(
                    &dir,
                    Limits::default(),
                    Box::new(SoftwareVault::generate().unwrap()),
                    Arc::new(crate::contacts::Book::open(&dir, Box::new(Contacts))),
                    Box::new(move |mail| {
                        opened.lock().unwrap().push(mail.clone());
                        Ok(())
                    }),
                )
                .unwrap()
            } else {
                Actions::with_vault(
                    &dir,
                    Limits::default(),
                    directory(),
                    Box::new(SoftwareVault::generate().unwrap()),
                )
                .unwrap()
            });
            let mut tasks = crate::task::Tasks::default();
            let (id, _) = tasks.begin("Omit \"SECRET-482\" from the email.", now());
            tasks.end(id, Ok("Understood.".into()), now());
            let cards = Arc::new(Mutex::new(Vec::new()));
            for (body, answer) in [
                ("PUB-391. SECRET-482 was omitted.", true),
                ("PUB-391", false),
                ("PUB-391", true),
            ] {
                let exclusions = tasks.email_exclusions(id);
                let proposed = call(
                    &serde_json::json!({"to":"Alex Rivera","subject":"Public update","body":body})
                        .to_string(),
                );
                let approved = actions.clone();
                let seen = cards.clone();
                let (ok, output) = tauri::async_runtime::block_on(actions.clone().run_checked(
                    &proposed,
                    move |mail| exclusions.check(mail),
                    move |request| {
                        assert_eq!(request.mail.body, "PUB-391");
                        seen.lock().unwrap().push(request.mail.clone());
                        approved.answer(&request.request, answer).unwrap();
                    },
                    Arc::new(AtomicBool::new(false)),
                    || Ok(()),
                ));
                assert_eq!(ok, body == "PUB-391" && answer, "{output}");
                if body.contains("SECRET") {
                    assert!(output.contains("excluded") && !output.contains("SECRET-482"));
                    assert!(cards.lock().unwrap().is_empty());
                }
                if !ok {
                    assert!(handed_off.lock().unwrap().is_empty());
                    assert!(!dir.join("outbox.jsonl").exists());
                }
            }
            let cards = cards.lock().unwrap();
            assert_eq!(
                cards.len(),
                2,
                "clean drafts still require an explicit decision"
            );
            assert_eq!(cards[0], cards[1]);
            if device {
                assert_eq!(*handed_off.lock().unwrap(), [cards[1].clone()]);
            } else {
                let rows = fs::read_to_string(dir.join("outbox.jsonl")).unwrap();
                assert_eq!(rows.lines().count(), 1);
                let sent: serde_json::Value = serde_json::from_str(rows.trim()).unwrap();
                assert_eq!(sent["body"], cards[1].body);
                assert_eq!(sent["subject"], cards[1].subject);
                assert!(!rows.contains("SECRET-482"));
            }
            assert!(verify(&read_log(&dir.join("audit.jsonl")).unwrap()).is_ok());
            fs::remove_dir_all(dir).unwrap();
        }
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
    fn the_system_prompt_names_the_recipients_before_the_subject() {
        let boss = Contact {
            name: "Sam Chen".to_string(),
            address: "sam@example.com".to_string(),
        };
        assert_eq!(
            reason_for(
                std::slice::from_ref(&boss),
                "Photos” to Mom, and “more” «x»"
            ),
            "send an email to Sam Chen: “Photos' to Mom, and 'more' 'x'”"
        );
        let long = "x".repeat(200);
        let reason = reason_for(&[boss], &long);
        assert!(reason.starts_with("send an email to Sam Chen: “"));
        assert_eq!(reason.chars().filter(|c| *c == 'x').count(), 60);
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
            ["send an email to Alex Rivera: “Lunch”"]
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
