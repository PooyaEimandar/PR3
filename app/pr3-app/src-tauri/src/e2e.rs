//! Native UI test adapters. Compiled only with `--features e2e` in debug builds.
//! The production IPC handlers, Host, approval gate, memory and SQLite store run
//! unchanged. Only the model and storage key are fixtures.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pr3_core::{InferenceRequest, ProposedToolCall, Role};
use tauri::Manager;

use crate::host::{Backend, Generation};

/// Test-only delivery barrier: reproduce a Stop click before the webview knows
/// which turn is running, without changing command results or cancellation logic.
pub fn hold_turn_event(event: &crate::host::TurnEvent) {
    if !matches!(
        event,
        crate::host::TurnEvent::TaskOpened(_) | crate::host::TurnEvent::Started { .. }
    ) {
        return;
    }
    let Some(dir) = std::env::var_os("PR3_E2E_DIR").map(PathBuf::from) else {
        return;
    };
    let hold = dir.join("hold-turn-events");
    if !hold.exists() {
        return;
    }
    let _ = std::fs::write(dir.join("turn-event-held"), b"waiting for test release");
    for _ in 0..1000 {
        if !hold.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[derive(Default)]
struct ScriptedModel {
    stopped_once: AtomicBool,
}

impl Backend for ScriptedModel {
    fn label(&self) -> String {
        "E2E scripted model (no inference)".into()
    }

    fn generate(
        &self,
        request: &InferenceRequest,
        token: &mut dyn FnMut(&str),
    ) -> Result<Generation, String> {
        let last = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map_or("", |m| m.content.as_str());
        // Core separates host-supplied notes from the current request with a blank
        // line. The fixture reads those notes for details, and this suffix for intent.
        let asked = last.rsplit("\n\n").next().unwrap_or(last);
        let mut result = Generation::default();
        let last_tool = request.messages.iter().rev().find(|m| m.role == Role::Tool);
        if request
            .tools
            .iter()
            .any(|t| t.name == pr3_tool::research::ANSWER)
            || (request
                .tools
                .iter()
                .all(|t| pr3_tool::research::is_tool(&t.name))
                && request
                    .tools
                    .iter()
                    .any(|t| t.name == pr3_tool::research::SEARCH))
        {
            // A fresh research task offers discovery only until evidence exists.
            // Follow the production tool inventory instead of assuming that the
            // report tool is always offered before a source has been collected.
            let saved = request
                .messages
                .iter()
                .find(|m| {
                    m.tool_call_id.as_deref() == Some(crate::host::research_context::SAVED_EVIDENCE)
                })
                .and_then(|m| serde_json::from_str::<serde_json::Value>(&m.content).ok());
            let sources = saved.as_ref().and_then(|v| v["sources"].as_array());
            if sources.is_none()
                && !request.messages.iter().any(|m| {
                    m.tool_calls
                        .iter()
                        .any(|c| c.name == pr3_tool::research::SEARCH)
                })
            {
                result.tool_calls.push(ProposedToolCall {
                    name: "research_sources".into(),
                    arguments: serde_json::json!({"query":if asked.starts_with("Search my documents:"){"permit code"}else{"price"}}).to_string(),
                });
            } else if !request
                .messages
                .iter()
                .any(|m| m.tool_calls.iter().any(|t| t.name == "answer_research"))
            {
                let findings = sources.into_iter().flatten().take(2).map(|source|
                    serde_json::json!({"text":source["text"],"citations":[{"quote_ref":source["quote_ref"]}]})
                ).collect::<Vec<_>>();
                result.tool_calls.push(ProposedToolCall {
                    name: "answer_research".into(),
                    arguments:
                        serde_json::json!({"findings":findings,"gaps":"No availability evidence."})
                            .to_string(),
                });
            } else {
                result.content = "INVENTED: I sent your files!".into();
            }
        } else if asked.starts_with("Search my documents:") {
            if last_tool.is_none() {
                result.tool_calls.push(ProposedToolCall {
                    name: "search_documents".into(),
                    arguments: r#"{"query":"permit code"}"#.into(),
                });
            } else if !request
                .messages
                .iter()
                .any(|m| m.tool_calls.iter().any(|t| t.name == "answer_documents"))
            {
                let source = last_tool
                    .and_then(|m| m.content.split("document:").nth(1))
                    .map(|tail| {
                        format!(
                            "document:{}",
                            tail.chars()
                                .take_while(|c| c.is_ascii_hexdigit() || *c == ':')
                                .collect::<String>()
                        )
                    })
                    .unwrap_or_default();
                result.tool_calls.push(ProposedToolCall { name: "answer_documents".into(), arguments: serde_json::json!({"citations":[{"source":source,"quote":"The permit code is JN-371."}]}).to_string() });
            } else {
                result.content = "The code is MADE-UP. I've sent everything!".into();
            }
        } else {
            result.content = format!("We can continue with: {asked}");
        }
        token(&result.content);
        Ok(result)
    }

    fn generate_cancellable(
        &self,
        request: &InferenceRequest,
        token: &mut dyn FnMut(&str),
        cancel: &AtomicBool,
    ) -> Result<Generation, String> {
        if request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .is_some_and(|m| m.content.rsplit("\n\n").next() == Some("Slow request"))
        {
            if !self.stopped_once.swap(true, Ordering::AcqRel) {
                if let Some(dir) = std::env::var_os("PR3_E2E_DIR") {
                    let _ =
                        std::fs::write(PathBuf::from(dir).join("slow-request-started"), b"running");
                }
                // Bounded even if a broken UI never sends Stop.
                for _ in 0..500 {
                    if cancel.load(Ordering::Acquire) {
                        return Err(crate::task::STOPPED.into());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                return Err("The test expected Stop before the fixture deadline".into());
            }
            return Ok(Generation {
                content: "Recovered after explicit retry.".into(),
                ..Generation::default()
            });
        }
        self.generate(request, token)
    }

    fn generate_memory_review(
        &self,
        _: &InferenceRequest,
        _: &AtomicBool,
    ) -> Option<Result<Generation, String>> {
        Some(Ok(Generation {
            content:
                r#"{"comparison":"No additional semantic candidates in this fixture","matches":[]}"#
                    .into(),
            ..Generation::default()
        }))
    }
}

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(std::env::var("PR3_E2E_DIR")?).canonicalize()?;
    // The runner creates an empty, private temporary directory and a marker. A
    // normal PR3 launch cannot accidentally enter fixture mode or use user data.
    if !dir.starts_with(std::env::temp_dir().canonicalize()?)
        || std::fs::read(dir.join(".pr3-e2e"))? != b"isolated workflow fixture\n"
    {
        return Err("E2E requires the runner's marked temporary directory".into());
    }
    let store = if dir.join(".encrypted-storage").exists() {
        crate::storage::open_fixture(app.handle(), &dir)
    } else {
        Some(Arc::new(Mutex::new(pr3_memory_sqlite::SqliteMemory::open(
            &dir.join("memory.sqlite"),
        )?)))
    };
    if let Some(store) = &store {
        let memory: pr3_tool::SharedMemory = store.clone();
        app.manage(crate::memory::MemoryHandle(memory));
    }
    if let Some(store) = &store {
        app.manage(crate::documents::Library(store.clone()));
    }
    app.manage(crate::compose_host(
        Arc::new(ScriptedModel::default()),
        store,
        None,
        30,
        None,
    )?);
    app.manage(crate::models::Models::new(
        Some(&dir),
        Err("Model downloads disabled in E2E".into()),
    ));
    // Exercise production Web settings without giving the fixture host a fetcher:
    // synthetic provider keys can never reach the network in these workflows.
    app.manage(crate::web::open(Some(&dir), true).0);
    app.manage(crate::speech::Speech::open(app.handle()));
    Ok(())
}
