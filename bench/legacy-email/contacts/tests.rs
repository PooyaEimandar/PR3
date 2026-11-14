use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

struct Fake {
    value: Arc<Mutex<Result<Snapshot, String>>>,
    requests: Arc<AtomicUsize>,
}
impl Source for Fake {
    fn read(&self, request: bool) -> Result<Snapshot, String> {
        if request {
            self.requests.fetch_add(1, Ordering::Relaxed);
        }
        self.value.lock().unwrap().clone()
    }
}

fn row(name: &str, address: &str) -> Contact {
    Contact {
        name: name.into(),
        address: address.into(),
    }
}
fn sample() -> Snapshot {
    Snapshot {
        permission: Permission::Granted,
        contacts: vec![row("Test Person", "test@example.com")],
    }
}
fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pr3-contacts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn book(dir: &Path, snapshot: Snapshot) -> (Arc<Book>, Fake) {
    let fake = Fake {
        value: Arc::new(Mutex::new(Ok(snapshot))),
        requests: Arc::new(AtomicUsize::new(0)),
    };
    let book = Book::open(
        dir,
        Box::new(Fake {
            value: fake.value.clone(),
            requests: fake.requests.clone(),
        }),
    );
    (Arc::new(book), fake)
}

#[test]
fn sync_is_opt_in_persists_only_preference_and_remove_survives_restart() {
    let dir = dir("opt-in");
    let (book, source) = book(&dir, sample());
    assert!(!book.refresh(false).unwrap().on);
    assert!(book.directory().is_err());
    assert_eq!(source.requests.load(Ordering::Relaxed), 0);
    let info = book.refresh(true).unwrap();
    assert!(info.on);
    assert_eq!(info.contacts, sample().contacts);
    let serialized = serde_json::to_string(&info).unwrap();
    assert!(!serialized.contains("test@example.com"));
    assert!(!serialized.contains("Test Person"));
    assert!(!serialized.contains("\"contacts\""));
    assert_eq!(
        std::fs::read_to_string(dir.join(ENABLED)).unwrap(),
        "enabled\n"
    );
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    let reopened = Book::open(
        &dir,
        Box::new(Fake {
            value: source.value.clone(),
            requests: source.requests.clone(),
        }),
    );
    assert_eq!(reopened.directory().unwrap().contacts, sample().contacts);
    assert_eq!(
        source.requests.load(Ordering::Relaxed),
        1,
        "refresh must not ask permission"
    );
    assert!(!book.disable().unwrap().on);
    assert!(book.info().unwrap().contacts.is_empty());
    let reopened = Book::open(&dir, Box::new(source));
    assert!(reopened.directory().is_err());
    assert!(reopened.info().unwrap().contacts.is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn denial_revocation_limited_access_and_provider_failure_never_leave_stale_contacts() {
    let dir = dir("permissions");
    let (book, source) = book(
        &dir,
        Snapshot {
            permission: Permission::Denied,
            contacts: sample().contacts,
        },
    );
    let denied = book.refresh(true).unwrap();
    assert!(!denied.on);
    assert!(denied.contacts.is_empty());
    assert!(!dir.join(ENABLED).exists());
    *source.value.lock().unwrap() = Ok(Snapshot {
        permission: Permission::Limited,
        contacts: sample().contacts,
    });
    assert_eq!(book.refresh(true).unwrap().permission, Permission::Limited);
    *source.value.lock().unwrap() = Err("provider offline".into());
    assert!(book.directory().is_err());
    assert!(book.info().unwrap().contacts.is_empty());
    *source.value.lock().unwrap() = Ok(Snapshot {
        permission: Permission::Denied,
        contacts: sample().contacts,
    });
    assert!(book.directory().is_err());
    assert!(!book.info().unwrap().on);
    assert!(!dir.join(ENABLED).exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn removing_contacts_invalidates_an_inflight_permission_response() {
    struct Slow {
        began: mpsc::Sender<()>,
        done: Mutex<mpsc::Receiver<()>>,
    }
    impl Source for Slow {
        fn read(&self, _: bool) -> Result<Snapshot, String> {
            self.began.send(()).unwrap();
            self.done.lock().unwrap().recv().unwrap();
            Ok(sample())
        }
    }
    let dir = dir("inflight");
    let (began_tx, began_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let book = Arc::new(Book::open(
        &dir,
        Box::new(Slow {
            began: began_tx,
            done: Mutex::new(done_rx),
        }),
    ));
    let pending = book.clone();
    let thread = std::thread::spawn(move || pending.refresh(true));
    began_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    book.disable().unwrap();
    done_tx.send(()).unwrap();
    assert!(thread.join().unwrap().is_err());
    assert!(!book.info().unwrap().on);
    assert!(book.info().unwrap().contacts.is_empty());
    assert!(!dir.join(ENABLED).exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn names_and_addresses_are_safe_and_multiple_emails_require_a_choice() {
    let input = vec![
        row("Sam, Chen", "sam.work@example.com"),
        row("Sam Chen", "sam.home@example.com"),
        row("Sam Chen", "sam.home@example.com"),
        row("", "unnamed@example.com"),
        row("Bad", "a@example.com,b@example.com"),
        row("Hidden\u{202e}", "a@example.com"),
        row("علی رضایی", "ali@example.com"),
    ];
    let contacts = normalize(input).unwrap();
    assert_eq!(contacts.len(), 3);
    assert_eq!(contacts[0].name, "Sam Chen [email 1]");
    assert_eq!(contacts[1].name, "Sam Chen [email 2]");
    assert_eq!(contacts[2].name, "علی رضایی");
    assert!(normalize(vec![row("A", "a@example.com"); MAX_CONTACTS + 1]).is_err());
    let directory = Directory { contacts };
    let validate = |to: &str| {
        use pr3_vault::Vault;
        let vault = pr3_vault::SoftwareVault::generate().unwrap();
        let key = pr3_core::trusted::ApprovalKey::from_sec1(&vault.public_key()).unwrap();
        let mut gate = pr3_core::trusted::Gate::new(
            Default::default(),
            key,
            "mail:device",
            &[],
            pr3_core::Timestamp(1),
        )
        .unwrap();
        gate.submit(
            &pr3_core::trusted::Envelope {
                actor: "agent:pr3".into(),
                principal: "user:local".into(),
                nonce: "n1".into(),
                received_at: pr3_core::Timestamp(1),
                proposal: pr3_core::trusted::Proposal {
                    operation: "mail.send".into(),
                    arguments: serde_json::json!({"to": to,"subject":"Hi","body":"Hello"})
                        .to_string(),
                },
            },
            &directory,
            pr3_core::Timestamp(1),
        )
    };
    assert!(matches!(
        validate("Sam Chen"),
        pr3_core::trusted::Submitted::Refused(reason) if reason.contains("could be")
    ));
    let pr3_core::trusted::Submitted::NeedsApproval(request) = validate("Sam Chen [email 2]")
    else {
        panic!("expected approval");
    };
    assert_eq!(request.mail.to[0].address, "sam.work@example.com");
    let pr3_core::trusted::Submitted::Refused(unknown) = validate("Missing") else {
        panic!("expected refusal");
    };
    let unknown = unknown.to_string();
    assert!(
        !unknown.contains("Sam"),
        "unknown lookups must not expose the address book"
    );
}

#[test]
fn device_email_uses_exact_approved_draft_and_never_claims_delivery() {
    let dir = dir("handoff");
    let (book, _) = book(&dir, sample());
    book.refresh(true).unwrap();
    let opened = Arc::new(Mutex::new(Vec::new()));
    let record = opened.clone();
    let actions = Arc::new(
        crate::actions::Actions::with_device(
            &dir,
            Default::default(),
            Box::new(pr3_vault::SoftwareVault::generate().unwrap()),
            book,
            Box::new(move |mail| {
                record.lock().unwrap().push(mail.clone());
                Ok(())
            }),
        )
        .unwrap(),
    );
    let call = pr3_core::ToolCall {
        id: "c1".into(),
        name: "send_email".into(),
        arguments:
            serde_json::json!({"to":"Test Person","subject":"سلام","body":"First\nSecond & third"})
                .to_string(),
    };
    let (success, result) = tauri::async_runtime::block_on(actions.clone().run(&call, |request| {
        assert_eq!(request.provider, crate::device_mail::PROVIDER);
        assert_eq!(request.mail.to, sample().contacts);
        actions.answer(&request.request, true).unwrap();
    }));
    assert!(success, "{result}");
    assert!(result.starts_with(crate::device_mail::OPENED));
    assert!(result.contains("not verified"));
    assert_eq!(opened.lock().unwrap().len(), 1);
    assert_eq!(opened.lock().unwrap()[0].body, "First\nSecond & third");
    assert!(!dir.join("outbox.jsonl").exists());
    let log = std::fs::read_to_string(dir.join("audit.jsonl")).unwrap();
    let entries: Vec<pr3_core::trusted::Entry> = log
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(pr3_core::trusted::verify(&entries), Ok(()));
    assert!(!log.contains("Second & third"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn removal_revocation_and_contact_changes_during_approval_block_handoff() {
    for mode in ["remove", "revoke", "edit", "decline", "no-app"] {
        let dir = dir(mode);
        let (book, source) = book(&dir, sample());
        book.refresh(true).unwrap();
        let opened = Arc::new(AtomicUsize::new(0));
        let count = opened.clone();
        let actions = Arc::new(
            crate::actions::Actions::with_device(
                &dir,
                Default::default(),
                Box::new(pr3_vault::SoftwareVault::generate().unwrap()),
                book.clone(),
                Box::new(move |_| {
                    count.fetch_add(1, Ordering::Relaxed);
                    if mode == "no-app" {
                        Err("No email app installed".into())
                    } else {
                        Ok(())
                    }
                }),
            )
            .unwrap(),
        );
        let call = pr3_core::ToolCall {
            id: "c1".into(),
            name: "send_email".into(),
            arguments: r#"{"to":"Test Person","subject":"Hi","body":"Hello"}"#.into(),
        };
        let (success, output) =
            tauri::async_runtime::block_on(actions.clone().run(&call, |request| {
                match mode {
                    "remove" => {
                        book.disable().unwrap();
                    }
                    "revoke" => {
                        source.value.lock().unwrap().as_mut().unwrap().permission =
                            Permission::Denied;
                    }
                    "edit" => {
                        source.value.lock().unwrap().as_mut().unwrap().contacts[0].address =
                            "changed@example.com".into();
                    }
                    _ => {}
                }
                actions.answer(&request.request, mode != "decline").unwrap();
            }));
        assert!(!success, "{mode}: {output}");
        assert!(!output.starts_with(crate::device_mail::OPENED));
        assert_eq!(
            opened.load(Ordering::Relaxed),
            usize::from(mode == "no-app")
        );
        assert!(!dir.join("outbox.jsonl").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

/// Local development evaluation: the only simulated pieces are the address book,
/// biometric key and email app. No real contact is read and no message is sent.
#[test]
#[ignore = "requires the installed Qwen3.5-4B model; runs three sampling seeds"]
fn installed_model_captures_corrects_resumes_and_opens_the_approved_draft() {
    use crate::host::{Backend, TurnEvent};
    use pr3_memory::{MemoryStore, Status};
    let model_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../model/Qwen3.5-4B-Q4_K_M.gguf");
    assert!(model_path.is_file());
    for seed in [1, 2, 7] {
        let started = std::time::Instant::now();
        let dir = dir(&format!("model-seed-{seed}"));
        let (book, _) = book(&dir, sample());
        book.refresh(true).unwrap();
        let backend: Arc<dyn Backend> =
            Arc::new(crate::llama::LlamaBackend::load_seeded(&model_path, seed).unwrap());
        let store = Arc::new(Mutex::new(
            pr3_memory_sqlite::SqliteMemory::open(&dir.join("memory.sqlite")).unwrap(),
        ));
        let first =
            crate::compose_host(backend.clone(), Some(store.clone()), None, 30, None).unwrap();
        tauri::async_runtime::block_on(first.send(
            "Remember that the project review meeting starts at 14:00.".into(),
            0,
            |_| {},
        ))
        .unwrap();
        let note = store
            .lock()
            .unwrap()
            .recent(10, crate::host::now())
            .unwrap()
            .into_iter()
            .find(|m| m.status == Status::Confirmed)
            .unwrap();
        tauri::async_runtime::block_on(first.send(
            format!(
                "Correct memory #{}: The project review meeting starts at 15:00.",
                note.id.0
            ),
            0,
            |_| {},
        ))
        .unwrap();
        let live = store
            .lock()
            .unwrap()
            .recent(10, crate::host::now())
            .unwrap();
        assert!(
            live.iter()
                .any(|m| m.status == Status::Confirmed && m.text.contains("15:00")),
            "correction must actually persist before proceeding"
        );
        assert!(
            !live
                .iter()
                .any(|m| m.status == Status::Confirmed && m.text.contains("14:00"))
        );
        let task_id = first.current_task().unwrap().unwrap().card.id;
        drop(first);
        drop(store);
        let opened = Arc::new(Mutex::new(Vec::new()));
        let record = opened.clone();
        let actions = Arc::new(
            crate::actions::Actions::with_device(
                &dir,
                Default::default(),
                Box::new(pr3_vault::SoftwareVault::generate().unwrap()),
                book,
                Box::new(move |mail| {
                    record.lock().unwrap().push(mail.clone());
                    Ok(())
                }),
            )
            .unwrap(),
        );
        let reopened = Arc::new(Mutex::new(
            pr3_memory_sqlite::SqliteMemory::open(&dir.join("memory.sqlite")).unwrap(),
        ));
        let host =
            crate::compose_host(backend, Some(reopened), Some(actions.clone()), 30, None).unwrap();
        assert_eq!(host.current_task().unwrap().unwrap().card.id, task_id);
        let approved = Arc::new(Mutex::new(Vec::new()));
        let cards = approved.clone();
        let events = Arc::new(Mutex::new(Vec::new()));
        let observed = events.clone();
        let turn_started = std::time::Instant::now();
        let result = tauri::async_runtime::block_on(host.send("Email Test Person the current project review meeting time from my confirmed memory. Use a short subject and body.".into(), 0, move |event| {
            if let TurnEvent::Approval { request, .. } = &event {
                cards.lock().unwrap().push(request.mail.clone());
                actions.answer(&request.request, true).unwrap();
            }
            observed.lock().unwrap().push(event);
        }));
        assert!(result.is_ok(), "seed {seed}: {result:?}");
        let drafts = opened.lock().unwrap();
        assert_eq!(drafts.len(), 1, "seed {seed}: {:?}", events.lock().unwrap());
        assert_eq!(
            *drafts,
            *approved.lock().unwrap(),
            "handoff must exactly match approval"
        );
        assert_eq!(drafts[0].to, sample().contacts);
        let body = drafts[0].body.to_lowercase();
        assert!(
            body.contains("15:00") || body.contains("3:00 pm") || body.contains("3 pm"),
            "seed {seed}: {body}"
        );
        assert!(
            !body.contains("14:00") && !body.contains("2:00 pm") && !body.contains("2 pm"),
            "seed {seed}: old time leaked: {body}"
        );
        let view = host.current_task().unwrap().unwrap();
        assert!(view.card.line.contains("email app"));
        assert!(!view.finished);
        assert!(events.lock().unwrap().iter().any(
            |e| matches!(e, TurnEvent::Result { text } if text.contains("delivery unverified"))
        ));
        println!(
            "email_workflow seed={seed} approved_drafts=1 turn_ms={} total_ms={} sending_verified=false",
            turn_started.elapsed().as_millis(),
            started.elapsed().as_millis()
        );
        drop(drafts);
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
