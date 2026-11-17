use super::chat::ChatControls;
use super::*;
use pr3_tool::research::{ANSWER, SEARCH};
use serde_json::json;

struct ChatModel {
    db: pr3_tool::documents::Library,
    mixed: bool,
}
impl Backend for ChatModel {
    fn label(&self) -> String {
        "chat routing fixture".into()
    }
    fn generate(
        &self,
        r: &InferenceRequest,
        _: &mut dyn FnMut(&str),
    ) -> Result<Generation, String> {
        let text = r
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .unwrap()
            .content
            .rsplit("\n\n")
            .next()
            .unwrap()
            .to_string();
        let call = |name: &str, args: Value| ProposedToolCall {
            name: name.into(),
            arguments: args.to_string(),
        };
        if text == "Hello" {
            return Ok(Generation {
                content: "Hello!".into(),
                ..Generation::default()
            });
        }
        if text == "What time is it?" {
            assert!(!r.tools.iter().any(|t| t.name == ANSWER));
            assert!(
                !r.messages
                    .iter()
                    .any(|m| m.content.contains("Research findings"))
            );
            return Ok(if r.messages.iter().any(|m| m.role == Role::Tool) {
                Generation {
                    content: "The clock returned the time.".into(),
                    ..Generation::default()
                }
            } else {
                Generation {
                    tool_calls: vec![call("get_time", json!({}))],
                    ..Generation::default()
                }
            });
        }
        if self.mixed && !r.tools.iter().any(|t| t.name == ANSWER) {
            return Ok(Generation {
                tool_calls: vec![
                    call("get_time", json!({})),
                    call(SEARCH, json!({"query":"price"})),
                ],
                ..Generation::default()
            });
        }
        let sources = self
            .db
            .lock()
            .unwrap()
            .research_sources(r.session.0)
            .unwrap();
        let tools = r.messages.iter().filter(|m| m.role == Role::Tool).count();
        let fresh = text.starts_with("Research ")
            && r.tools.iter().any(|t| t.name == SEARCH)
            && !r
                .messages
                .iter()
                .any(|m| m.tool_calls.iter().any(|c| c.name == SEARCH));
        let next = if fresh
            || !r.tools.iter().any(|t| t.name == ANSWER)
            || (sources.is_empty() && tools == 0)
        {
            call(
                SEARCH,
                json!({"query":if fresh || sources.is_empty(){"price"}else{""}}),
            )
        } else {
            call(
                ANSWER,
                json!({"findings":sources.iter().take(2).map(|s|json!({"text":s.text,"citations":[{"source":s.source,"quote":s.text}]})).collect::<Vec<_>>(),"gaps":if sources.is_empty(){"No supporting source was available."}else{""}}),
            )
        };
        Ok(Generation {
            tool_calls: vec![next],
            ..Generation::default()
        })
    }
}
#[derive(Default)]
struct SearchLog(Mutex<Vec<String>>);
impl pr3_tool::research::SearchWeb for SearchLog {
    fn search(&self, topic: &str) -> Result<Vec<pr3_tool::research::SearchHit>, String> {
        self.0.lock().unwrap().push(topic.into());
        Ok(vec![])
    }
}
fn make_host(db: pr3_tool::documents::Library, search: Arc<SearchLog>, mixed: bool) -> Host {
    let mut tools = pr3_tool::research::tools(db.clone(), None, Some(search));
    tools.push(Box::new(pr3_tool::Clock));
    Host::new(
        Arc::new(ChatModel {
            db: db.clone(),
            mixed,
        }),
        tools,
    )
    .unwrap()
    .with_tasks(db, 30)
    .unwrap()
}
fn library() -> pr3_tool::documents::Library {
    let mut db = SqliteMemory::open_in_memory().unwrap();
    db.import_document("alpha.md", "Alpha price is 25.")
        .unwrap();
    Arc::new(Mutex::new(db))
}
fn chat(host: &Host, text: &str, controls: ChatControls) -> (String, Vec<TurnEvent>) {
    let events = Arc::new(Mutex::new(vec![]));
    let sink = events.clone();
    let actions =
        tauri::async_runtime::block_on(host.send_chat(text.into(), controls, 0, move |e| {
            sink.lock().unwrap().push(e)
        }))
        .unwrap();
    let reply = actions
        .into_iter()
        .find_map(|a| {
            if let Action::Reply { content, .. } = a {
                Some(content)
            } else {
                None
            }
        })
        .unwrap();
    let events = events.lock().unwrap().clone();
    (reply, events)
}
#[test]
fn natural_research_and_ordinary_tools_share_a_task_and_saved_sources_survive_reopen() {
    let dir = std::env::temp_dir().join(format!("pr3-chat-reopen-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("db.sqlite");
    let db = Arc::new(Mutex::new(SqliteMemory::open(&path).unwrap()));
    db.lock()
        .unwrap()
        .import_document("alpha.md", "Alpha price is 25.")
        .unwrap();
    let search = Arc::new(SearchLog::default());
    let host = make_host(db.clone(), search.clone(), false);
    assert_eq!(chat(&host, "Hello", ChatControls::default()).0, "Hello!");
    let id = host.current_task().unwrap().unwrap().card.id;
    assert!(db.lock().unwrap().research(id).unwrap().is_none());
    assert!(
        chat(&host, "Compare these documents", ChatControls::default())
            .0
            .contains("Alpha price is 25.")
    );
    assert_eq!(
        chat(&host, "What time is it?", ChatControls::default()).0,
        "The clock returned the time."
    );
    assert_eq!(host.current_task().unwrap().unwrap().card.id, id);
    assert_eq!(host.task_cards().unwrap().len(), 1);
    drop(host);
    drop(db);
    let db = Arc::new(Mutex::new(SqliteMemory::open(&path).unwrap()));
    let host = make_host(db.clone(), search.clone(), false);
    host.continue_task(SessionId(id)).unwrap();
    let (answer, events) = chat(&host, "Now quote the saved price", ChatControls::default());
    assert!(answer.contains("Alpha price is 25."));
    assert!(!events.iter().any(
        |e| matches!(e,TurnEvent::Tool{name,..}if name==SEARCH||name==pr3_tool::research::READ)
    ));
    assert!(search.0.lock().unwrap().is_empty());
    host.new_topic().unwrap();
    chat(&host, "Hello", ChatControls::default());
    let other = host.current_task().unwrap().unwrap().card.id;
    assert_ne!(id, other);
    assert!(
        db.lock()
            .unwrap()
            .research_sources(other)
            .unwrap()
            .is_empty()
    );
    host.delete_task(SessionId(id)).unwrap();
    assert!(
        db.lock()
            .unwrap()
            .chat_request(id, "Now quote the saved price")
            .unwrap()
            .is_none()
    );
    drop(host);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn model_selection_enters_research_before_any_mixed_batch_tool_runs() {
    let db = library();
    let host = make_host(db, Arc::new(SearchLog::default()), true);
    let (answer, events) = chat(
        &host,
        "Which plan suits this budget?",
        ChatControls::default(),
    );
    assert!(answer.contains("Alpha price is 25."));
    assert!(
        events
            .iter()
            .any(|e| matches!(e,TurnEvent::Tool{name,success:false,..}if name=="get_time"))
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e,TurnEvent::Tool{name,success:true,..}if name=="get_time"))
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, TurnEvent::Approval { .. } | TurnEvent::Token { .. }))
    );
}
#[test]
fn web_requires_composer_permission_and_uses_only_the_actual_message() {
    let db = library();
    let search = Arc::new(SearchLog::default());
    let host = make_host(db.clone(), search.clone(), false);
    chat(
        &host,
        "Research online: Compare prices",
        ChatControls::default(),
    );
    assert!(search.0.lock().unwrap().is_empty());
    let id = host.current_task().unwrap().unwrap().card.id;
    chat(
        &host,
        "Research the latest plans",
        ChatControls {
            web: true,
            websites: String::new(),
        },
    );
    assert_eq!(*search.0.lock().unwrap(), ["Research the latest plans"]);
    assert_eq!(host.current_task().unwrap().unwrap().card.id, id);
    chat(
        &host,
        "Now quote the saved price; do not fetch",
        ChatControls {
            web: true,
            websites: String::new(),
        },
    );
    assert_eq!(search.0.lock().unwrap().len(), 1);
    assert!(!db.lock().unwrap().research(id).unwrap().unwrap().web);
}
#[test]
fn inline_scope_is_validated_before_task_creation_and_survives_changed_questions() {
    let db = library();
    let host = make_host(db.clone(), Arc::new(SearchLog::default()), false);
    assert!(
        tauri::async_runtime::block_on(host.send_chat(
            "Research plans".into(),
            ChatControls {
                web: true,
                websites: "https://example.org".into()
            },
            0,
            |_| {}
        ))
        .is_err()
    );
    assert!(host.task_cards().unwrap().is_empty());
    chat(
        &host,
        "Research plans",
        ChatControls {
            web: true,
            websites: "example.org".into(),
        },
    );
    let id = host.current_task().unwrap().unwrap().card.id;
    chat(
        &host,
        "Research support terms",
        ChatControls {
            web: true,
            websites: "docs.example.org".into(),
        },
    );
    assert_eq!(host.chat_scope().unwrap(), ["docs.example.org"]);
    chat(
        &host,
        "Research pricing again",
        ChatControls {
            web: true,
            websites: String::new(),
        },
    );
    assert_eq!(host.chat_scope().unwrap(), ["docs.example.org"]);
    assert!(
        db.lock()
            .unwrap()
            .research(id)
            .unwrap()
            .unwrap()
            .topic
            .starts_with("Research pricing again Sources only: docs.example.org")
    );
    let failed = tauri::async_runtime::block_on(host.send_chat(
        "Research other plans".into(),
        ChatControls {
            web: true,
            websites: "other.example".into(),
        },
        0,
        |_| {},
    ));
    assert!(failed.is_err());
    assert_eq!(host.chat_scope().unwrap(), ["docs.example.org"]);
}
#[test]
fn retry_restores_only_the_controls_of_that_request() {
    let db = library();
    let search = Arc::new(SearchLog::default());
    let host = Arc::new(make_host(db.clone(), search.clone(), false));
    let control = host.clone();
    let actions = tauri::async_runtime::block_on(host.send_chat(
        "Research prices".into(),
        ChatControls {
            web: true,
            websites: String::new(),
        },
        0,
        move |event| {
            if matches!(event,TurnEvent::Tool{ref name,..}if name==SEARCH) {
                let id = control.current_task().unwrap().unwrap().card.id;
                control.stop_task(SessionId(id)).unwrap();
            }
        },
    ))
    .unwrap();
    assert!(matches!(actions.as_slice(), [Action::Failed { .. }]));
    let id = host.current_task().unwrap().unwrap().card.id;
    drop(host);
    let host = make_host(db.clone(), search.clone(), false);
    let actions = tauri::async_runtime::block_on(host.retry(SessionId(id), 0, |_| {})).unwrap();
    assert!(
        actions.iter().any(
            |a| matches!(a,Action::Reply{content,..}if content.contains("Alpha price is 25."))
        )
    );
    assert!(db.lock().unwrap().research(id).unwrap().unwrap().web);
    assert!(
        db.lock()
            .unwrap()
            .chat_request(id, "Different message")
            .unwrap()
            .is_none()
    );
    chat(&host, "Hello", ChatControls::default());
    assert!(
        db.lock()
            .unwrap()
            .chat_request(id, "Research prices")
            .unwrap()
            .is_none()
    );
}

#[test]
fn retry_web_control_can_revoke_but_cannot_upgrade_permission() {
    for original_web in [false, true] {
        let db = library();
        let search = Arc::new(SearchLog::default());
        let host = Arc::new(make_host(db.clone(), search.clone(), false));
        let control = host.clone();
        tauri::async_runtime::block_on(host.send_chat(
            "Research prices".into(),
            ChatControls {
                web: original_web,
                websites: String::new(),
            },
            0,
            move |event| {
                if matches!(event,TurnEvent::Tool{ref name,..}if name==SEARCH) {
                    let id = control.current_task().unwrap().unwrap().card.id;
                    control.stop_task(SessionId(id)).unwrap();
                }
            },
        ))
        .unwrap();
        let id = host.current_task().unwrap().unwrap().card.id;
        let searched = search.0.lock().unwrap().len();
        let actions = tauri::async_runtime::block_on(host.retry_with_controls(
            SessionId(id),
            Some(ChatControls {
                web: !original_web,
                websites: String::new(),
            }),
            0,
            |_| {},
        ))
        .unwrap();
        assert!(actions.iter().any(
            |a| matches!(a,Action::Reply{content,..}if content.contains("Alpha price is 25."))
        ));
        assert!(!db.lock().unwrap().research(id).unwrap().unwrap().web);
        assert_eq!(search.0.lock().unwrap().len(), searched);
    }
}
