//! Real database, host and approval checks; scripted drafts deliberately disclose
//! excluded content so the test verifies enforcement independently of model quality.
use super::*;
use std::collections::VecDeque;

struct Scripted(Mutex<VecDeque<Generation>>);
impl Backend for Scripted {
    fn label(&self) -> String {
        "email meaning fixture".into()
    }
    fn generate(
        &self,
        _: &InferenceRequest,
        _: &mut dyn FnMut(&str),
    ) -> Result<Generation, String> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("no extra inference or automatic action retry"))
    }
}
fn answer() -> Generation {
    Generation {
        content: "The email was sent.".into(),
        ..Generation::default()
    }
}
fn draft(body: &str) -> Generation {
    Generation {
        tool_calls: vec![ProposedToolCall {
            name: "send_email".into(),
            arguments: serde_json::json!({"to":"Alex", "subject":"Public update", "body":body})
                .to_string(),
        }],
        ..Generation::default()
    }
}
fn runtime(dir: &std::path::Path, script: Vec<Generation>) -> (Host, Arc<Actions>) {
    runtime_with_backend(dir, Arc::new(Scripted(Mutex::new(script.into()))))
}
fn runtime_with_backend(dir: &std::path::Path, backend: Arc<dyn Backend>) -> (Host, Arc<Actions>) {
    let actions = Arc::new(
        Actions::with_vault(
            dir,
            Default::default(),
            pr3_core::trusted::Directory {
                contacts: vec![pr3_core::trusted::Contact {
                    name: "Alex".into(),
                    address: "alex@example.com".into(),
                }],
            },
            Box::new(pr3_vault::SoftwareVault::generate().unwrap()),
        )
        .unwrap(),
    );
    let host = Host::new(backend, pr3_tool::builtin())
        .unwrap()
        .with_tasks(
            Arc::new(Mutex::new(
                SqliteMemory::open(&dir.join("memory.sqlite")).unwrap(),
            )),
            30,
        )
        .unwrap()
        .with_actions(actions.clone())
        .unwrap();
    (host, actions)
}

#[test]
fn semantic_exclusions_reincluded_value_reaches_the_model_after_restart() {
    struct Inspect;
    impl Backend for Inspect {
        fn label(&self) -> String {
            "Inspect restart context".into()
        }
        fn generate(
            &self,
            request: &InferenceRequest,
            _: &mut dyn FnMut(&str),
        ) -> Result<Generation, String> {
            let current = request
                .messages
                .iter()
                .rev()
                .find(|m| m.role == pr3_core::Role::User)
                .unwrap();
            assert!(
                current.content.contains("re-included"),
                "{}",
                current.content
            );
            assert!(
                current.content.contains("salary: 74200"),
                "{}",
                current.content
            );
            Ok(Generation {
                content: "Ready for review.".into(),
                ..Generation::default()
            })
        }
    }
    let dir = std::env::temp_dir().join(format!("pr3-semantic-context-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (host, actions) = runtime(&dir, vec![]);
    {
        let mut tasks = host.tasks().unwrap();
        let (id, _) = tasks.begin("Prepare an email later to Alex Rivera about my salary update. Public reference PUB-391. My salary is 74200 USD; my bank account is ACCT-648. Wait before drafting.", now());
        tasks.end(
            id,
            Ok("1 memory note saved for review · No email sent".into()),
            now(),
        );
    }
    quiet(&host, "Keep my financial details out of the email.");
    quiet(&host, "Include my income in the email.");
    drop(host);
    drop(actions);
    let (host, actions) = runtime_with_backend(&dir, Arc::new(Inspect));
    tauri::async_runtime::block_on(host.send("Prepare the salary update email to Alex now. Include my income and the public reference.".into(), 0, |_| {})).unwrap();
    drop(host);
    drop(actions);
    std::fs::remove_dir_all(dir).unwrap();
}
fn quiet(host: &Host, text: &str) -> Vec<Action> {
    tauri::async_runtime::block_on(host.send(text.into(), 0, |e| {
        assert!(
            !matches!(
                e,
                TurnEvent::Approval { .. } | TurnEvent::Tool { .. } | TurnEvent::Token { .. }
            ),
            "a correction cannot prepare or send an email"
        )
    }))
    .unwrap()
}
fn reply(actions: &[Action]) -> &str {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Reply { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn semantic_exclusions_block_restored_and_generated_cards_but_keep_exact_fresh_approval() {
    for (pending, approve) in [(false, false), (false, true), (true, false), (true, true)] {
        let dir = std::env::temp_dir().join(format!(
            "pr3-semantic-card-{pending}-{approve}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (first, actions) = runtime(&dir, vec![]);
        let id = {
            let mut tasks = first.tasks().unwrap();
            let (id, _) = tasks.begin("Email Alex later: My salary is 74200 USD; my diagnosis is asthma. Public reference PUB-391.", now());
            tasks.end(id, Ok("Waiting.".into()), now());
            id
        };
        assert!(
            reply(&quiet(&first, "اطلاعات مالی من را از ایمیل حذف کن."))
                .contains("No email was prepared or sent")
        );
        quiet(&first, "No incluyas mis datos médicos en el correo.");
        if pending {
            let mut tasks = first.tasks().unwrap();
            tasks.begin("Prepare the email.", now());
            tasks.waiting(
                id,
                &ToolCall {
                    id: "old-draft".into(),
                    name: "send_email".into(),
                    arguments: draft("Salary 74200 USD omitted; asthma.").tool_calls[0]
                        .arguments
                        .clone(),
                },
                Timestamp(now().0 + 60_000),
                now(),
            );
        }
        drop(first);
        drop(actions);
        let mut script = Vec::new();
        if !pending {
            script.extend([draft("Salary 74200 USD omitted; asthma."), answer()]);
        }
        script.extend([draft("Public reference PUB-391."), answer()]);
        let (host, actions) = runtime(&dir, script);
        if pending {
            let error = tauri::async_runtime::block_on(
                host.ask_again(id, |e| assert!(!matches!(e, TurnEvent::Approval { .. }))),
            )
            .unwrap_err();
            assert!(error.contains("NOT SENT"));
        } else {
            tauri::async_runtime::block_on(host.send("Prepare the email now.".into(), 0, |e| {
                assert!(!matches!(e, TurnEvent::Approval { .. }))
            }))
            .unwrap();
        }
        let outbox = dir.join("outbox.jsonl");
        assert!(!outbox.exists());
        let before = outbox.clone();
        let decision = actions.clone();
        let cards = Arc::new(Mutex::new(Vec::new()));
        let seen = cards.clone();
        let replies = tauri::async_runtime::block_on(host.send(
            "Prepare the email with only the public reference now.".into(),
            0,
            move |e| {
                if let TurnEvent::Approval { request, .. } = e {
                    assert!(!before.exists());
                    assert_eq!(request.mail.to[0].address, "alex@example.com");
                    assert_eq!(request.mail.body, "Public reference PUB-391.");
                    seen.lock().unwrap().push(request.mail);
                    decision.answer(&request.request, approve).unwrap();
                }
            },
        ))
        .unwrap();
        assert_eq!(cards.lock().unwrap().len(), 1);
        assert_eq!(outbox.exists(), approve);
        if approve {
            let text = std::fs::read_to_string(&outbox).unwrap();
            assert_eq!(text.lines().count(), 1);
            let sent: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
            let approved = cards.lock().unwrap();
            assert_eq!(sent["body"], approved[0].body);
            assert_eq!(sent["subject"], approved[0].subject);
            assert_eq!(sent["to"], serde_json::json!(["Alex <alex@example.com>"]));
        } else {
            assert!(reply(&replies).contains("No email sent"));
        }
        drop(host);
        drop(actions);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn semantic_exclusions_clarification_survives_restart_and_failed_save_cannot_reinclude() {
    let dir = std::env::temp_dir().join(format!("pr3-semantic-clarify-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("memory.sqlite");
    let (host, actions) = runtime(&dir, vec![]);
    {
        let mut tasks = host.tasks().unwrap();
        let (id, _) = tasks.begin(
            "Email Alex later: My salary is 74200 USD; public reference PUB-391.",
            now(),
        );
        tasks.end(id, Ok("Waiting.".into()), now());
    }
    quiet(&host, "Keep my financial details out of the email.");
    assert!(
        reply(&quiet(
            &host,
            "Leave out that personal detail from the email."
        ))
        .contains("quote the exact text")
    );
    let id = SessionId(host.current_task().unwrap().unwrap().card.id);
    drop(host);
    drop(actions);
    let (host, actions) = runtime(&dir, vec![]);
    assert!(reply(&quiet(&host, "By that I mean 'PRIVATE-2'.")).contains("instructions updated"));
    let check = |body: &str| pr3_core::trusted::Mail {
        to: Vec::new(),
        subject: "Update".into(),
        body: body.into(),
    };
    assert!(
        host.tasks()
            .unwrap()
            .email_exclusions(id)
            .check(&check("PUB-391"))
            .is_ok()
    );
    assert!(
        host.tasks()
            .unwrap()
            .email_exclusions(id)
            .check(&check("PRIVATE-2"))
            .is_err()
    );
    let injector = rusqlite::Connection::open(&path).unwrap();
    injector.execute_batch("CREATE TRIGGER fail_meaning BEFORE INSERT ON messages BEGIN SELECT RAISE(FAIL, 'storage full'); END;").unwrap();
    assert!(
        tauri::async_runtime::block_on(host.send(
            "Incluye mis datos financieros en el correo.".into(),
            0,
            |_| {}
        ))
        .is_err()
    );
    drop(host);
    drop(actions);
    injector
        .execute_batch("DROP TRIGGER fail_meaning;")
        .unwrap();
    drop(injector);
    let (host, actions) = runtime(&dir, vec![]);
    assert!(
        host.tasks()
            .unwrap()
            .email_exclusions(id)
            .check(&check("74200"))
            .is_err()
    );
    assert!(
        host.tasks()
            .unwrap()
            .email_exclusions(id)
            .check(&check("PRIVATE-2"))
            .is_err()
    );
    drop(host);
    drop(actions);
    std::fs::remove_dir_all(dir).unwrap();
}
