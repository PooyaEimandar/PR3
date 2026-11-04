//! Native UI test adapters. Compiled only with `--features e2e` in debug builds.
//! The production IPC handlers, Host, approval gate, memory and SQLite store run
//! unchanged. Only the model, address book, key and mail-app handoff are fixtures.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pr3_core::{InferenceRequest, ProposedToolCall, Role};
use tauri::Manager;

use crate::host::{Backend, Generation};

struct AddressBook;

impl crate::contacts::Source for AddressBook {
    fn read(&self, _: bool) -> Result<crate::contacts::Snapshot, String> {
        serde_json::from_value(serde_json::json!({
            "permission": "granted",
            "contacts": [{"name": "Alex Rivera", "address": "alex@example.com"}]
        }))
        .map_err(|e| e.to_string())
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
        if request
            .messages
            .last()
            .is_some_and(|m| m.role == Role::Tool)
        {
            // Deliberately false: the production result guard must replace it.
            result.content = "I've sent your email. Everything is completed!".into();
        } else if asked == "Email Alex the meeting details." {
            // Read the persisted conversation. Missing/obsolete context fails the
            // UI assertion; the fixture does not restore it from a test-side value.
            let time = request
                .messages
                .iter()
                .rev()
                .filter(|m| m.role == Role::User)
                .find_map(|m| {
                    ["15:00", "14:00"]
                        .into_iter()
                        .find(|v| m.content.contains(v))
                })
                .unwrap_or("MISSING CONTEXT");
            result.tool_calls.push(ProposedToolCall {
                name: "send_email".into(),
                arguments: serde_json::json!({"to":"Alex", "subject":"Meeting", "body":format!("The meeting is at {time}.")}).to_string(),
            });
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
            .last()
            .is_some_and(|m| m.content.rsplit("\n\n").next() == Some("Slow request"))
        {
            if !self.stopped_once.swap(true, Ordering::AcqRel) {
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
    let store = Arc::new(Mutex::new(pr3_memory_sqlite::SqliteMemory::open(
        &dir.join("memory.sqlite"),
    )?));
    let memory: pr3_tool::SharedMemory = store.clone();
    app.manage(crate::memory::MemoryHandle(memory));
    let book = Arc::new(crate::contacts::Book::open(&dir, Box::new(AddressBook)));
    app.manage(crate::contacts::Handle(book.clone()));
    let mail_dir = dir.clone();
    let actions = crate::actions::Actions::with_device(
        &dir,
        Default::default(),
        Box::new(pr3_vault::SoftwareVault::open(
            &dir.join(pr3_vault::SOFTWARE_KEY_FILE),
        )?),
        book,
        Box::new(move |mail| {
            if mail_dir.join("mail.fail").exists() {
                return Err("Fixture mail app refused the draft".into());
            }
            let mut out = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(mail_dir.join("handoffs.jsonl"))
                .map_err(|e| e.to_string())?;
            writeln!(
                out,
                "{}",
                serde_json::to_string(mail).map_err(|e| e.to_string())?
            )
            .and_then(|_| out.sync_all())
            .map_err(|e| e.to_string())
        }),
    )?;
    app.manage(crate::compose_host(
        Arc::new(ScriptedModel::default()),
        Some(store),
        Some(Arc::new(actions)),
        30,
        None,
    )?);
    app.manage(crate::models::Models::new(
        Some(&dir),
        Err("Model downloads disabled in E2E".into()),
    ));
    app.manage(crate::web::open(Some(&dir), false).0);
    app.manage(crate::speech::Speech::open(app.handle()));
    Ok(())
}
