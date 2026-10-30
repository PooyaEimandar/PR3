//! Host adapter: runs `pr3-core` inference requests on a bundled GGUF model.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use pr3_core::{GenerationInfo, InferenceRequest, ProposedToolCall, Role};
use pr3_llamacpp::{
    ChatMessage, ChatRole, Engine, EngineConfig, EngineError, ToolCallText, ToolDef,
};

use crate::host::{Backend, Generation};

const SYSTEM_PROMPT: &str = "You are PR3, a private assistant running entirely on the \
user's own device. Answer clearly and briefly. Use the tools for calculations and the \
current time instead of guessing.";

/// Added only when the memory tools are offered.
const MEMORY_PROMPT: &str = " When the user tells you something about themselves worth \
keeping, or asks you to remember something, save it with remember; before answering \
questions about the user, look it up with recall.";

/// Added only when sending email is offered.
const MAIL_PROMPT: &str = " To send an email, call send_email with the contacts' names \
and the complete subject and text; PR3 shows the user the exact email and sends it only \
if they approve. Say an email was sent only if send_email answers SENT; if it answers \
NOT SENT, tell the user it was not sent.";

pub struct LlamaBackend {
    engine: Engine,
}

impl LlamaBackend {
    pub fn load(path: &Path) -> Result<Self, EngineError> {
        Self::load_seeded(path, seed())
    }

    /// Like `load`, but sampling from `seed`, so a benchmark run can be repeated.
    pub fn load_seeded(path: &Path, seed: u32) -> Result<Self, EngineError> {
        let config = EngineConfig {
            // Metal on Macs and iPhones. CPU on Android until a Vulkan build is measured
            // per device class (docs Sec. 4.3), and in the iOS simulator, whose Metal
            // produces wrong results for llama.cpp's kernels.
            gpu_layers: if cfg!(any(target_os = "android", target_abi = "sim")) {
                0
            } else {
                999
            },
            seed,
            ..EngineConfig::default()
        };
        Ok(Self {
            engine: Engine::load(path, config)?,
        })
    }
}

impl Backend for LlamaBackend {
    fn label(&self) -> String {
        self.engine.label().to_string()
    }

    fn stop(&self) {
        self.engine.stop();
    }

    fn generate(
        &self,
        request: &InferenceRequest,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<Generation, String> {
        self.generate_until(request, &mut |text| {
            on_token(text);
            true
        })
    }

    fn generate_until(
        &self,
        request: &InferenceRequest,
        on_token: &mut dyn FnMut(&str) -> bool,
    ) -> Result<Generation, String> {
        let mut messages = Vec::with_capacity(request.messages.len() + 1);
        let mut system = SYSTEM_PROMPT.to_string();
        if request.tools.iter().any(|t| t.name == "remember") {
            system.push_str(MEMORY_PROMPT);
        }
        if request
            .tools
            .iter()
            .any(|t| t.name == pr3_tool::mail::SEND_EMAIL)
        {
            system.push_str(MAIL_PROMPT);
        }
        messages.push(ChatMessage::new(ChatRole::System, system));
        messages.extend(request.messages.iter().map(|m| {
            let role = match m.role {
                Role::System => ChatRole::System,
                Role::User => ChatRole::User,
                Role::Assistant => ChatRole::Assistant,
                Role::Tool => ChatRole::Tool,
            };
            ChatMessage {
                // The engine replays earlier calls in the model's own format.
                tool_calls: m
                    .tool_calls
                    .iter()
                    .map(|c| ToolCallText {
                        name: c.name.clone(),
                        arguments: c.arguments.clone(),
                    })
                    .collect(),
                ..ChatMessage::new(role, m.content.clone())
            }
        }));
        let tools: Vec<ToolDef> = request
            .tools
            .iter()
            .map(|t| ToolDef {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            })
            .collect();

        let out = self
            .engine
            .chat_with_tools_until(&messages, &tools, on_token)
            .map_err(|e| e.to_string())?;
        Ok(Generation {
            content: out.text,
            tool_calls: out
                .tool_calls
                .into_iter()
                .map(|c| ProposedToolCall {
                    name: c.name,
                    arguments: c.arguments,
                })
                .collect(),
            info: GenerationInfo {
                model: self.engine.label().to_string(),
                engine: "llama.cpp".to_string(),
                prompt_tokens: out.prompt_tokens,
                completion_tokens: out.completion_tokens,
            },
        })
    }
}

/// The repository's `model/` folder, where debug builds also look for models. Debug
/// builds and tests only, so release builds carry no path from the build machine.
#[cfg(any(debug_assertions, test))]
pub fn repo_models() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../model")
}

/// Varies answers between launches; the core stays deterministic either way.
fn seed() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use pr3_core::Action;

    use super::*;
    use crate::actions::Actions;
    use crate::config::Config;
    use crate::host::{Backend, Host, TurnEvent};
    use pr3_vault::SoftwareVault;

    fn host(path: &Path) -> Host {
        Host::new(
            Arc::new(LlamaBackend::load(path).unwrap()),
            pr3_tool::builtin(),
        )
        .unwrap()
    }

    fn turn(host: &Host, text: &str) -> (String, Vec<TurnEvent>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let actions = tauri::async_runtime::block_on(
            host.send(text.to_string(), 0, move |e| sink.lock().unwrap().push(e)),
        )
        .unwrap();
        let [Action::Reply { content, .. }] = actions.as_slice() else {
            panic!("expected one reply, got {actions:?}");
        };
        let events = events.lock().unwrap().clone();
        (content.clone(), events)
    }

    fn tool_names(events: &[TurnEvent]) -> Vec<&str> {
        events
            .iter()
            .filter_map(|e| match e {
                TurnEvent::Tool { name, .. } => Some(name.as_str()),
                TurnEvent::Token { .. }
                | TurnEvent::Approval { .. }
                | TurnEvent::TaskOpened(_)
                | TurnEvent::NewTask
                | TurnEvent::Summary { .. } => None,
            })
            .collect()
    }

    /// Runs real turns through core, host, Qwen and the built-in tools, for each model
    /// config/pr3.toml selects (desktop, mobile, small mobile) that is in model/.
    /// Opt-in because it needs them:
    /// script/fetch-model.sh && cargo test -p pr3-app --release -- --ignored --nocapture
    #[test]
    #[ignore = "needs the selected models in model/"]
    fn real_models_answer_and_use_the_tools() {
        let config = Config::built_in().unwrap();
        let choice = &config.model;
        let mut tested = 0;
        for id in [&choice.desktop, &choice.mobile, &choice.mobile_small] {
            let path = repo_models().join(&config.models[id].file);
            if !path.is_file() {
                eprintln!("{id}: {} missing, skipped", path.display());
                continue;
            }
            let host = host(&path);
            let (answer, _) = turn(&host, "What is the capital of France? Answer in one word.");
            assert!(answer.contains("Paris"), "{id}: {answer}");
            assert!(!answer.contains("<think>") && !answer.contains("<tool_call>"));

            let (answer, events) = turn(&host, "What is 1234 * 5678?");
            let used_calculator = events.iter().any(|e| {
                matches!(e, TurnEvent::Tool { name, output, success: true, .. }
                    if name == "calculate" && output == "7006652")
            });
            assert!(used_calculator, "{id}: {events:?}");
            assert!(
                answer.replace(',', "").contains("7006652"),
                "{id}: {answer}"
            );

            let (answer, events) = turn(&host, "What's today's date?");
            assert_eq!(tool_names(&events), ["get_time"], "{id}: {answer}");
            eprintln!("{id}: ok ({answer})");
            tested += 1;
        }
        assert!(
            tested > 0,
            "no selected model in {}",
            repo_models().display()
        );
    }

    /// Tells one conversation a fact and asks for it in a new one that shares only the
    /// memory store, so only long-term memory can carry it across.
    #[test]
    #[ignore = "needs the selected models in model/"]
    fn real_models_remember_across_conversations() {
        let config = Config::built_in().unwrap();
        let choice = &config.model;
        let mut tested = 0;
        for id in [&choice.desktop, &choice.mobile, &choice.mobile_small] {
            let path = repo_models().join(&config.models[id].file);
            if !path.is_file() {
                eprintln!("{id}: {} missing, skipped", path.display());
                continue;
            }
            let file =
                std::env::temp_dir().join(format!("pr3-memory-{id}-{}.sqlite", std::process::id()));
            let _ = std::fs::remove_file(&file);
            let store: pr3_tool::SharedMemory = Arc::new(Mutex::new(
                pr3_memory_sqlite::SqliteMemory::open(&file).unwrap(),
            ));
            let backend: Arc<dyn Backend> = Arc::new(LlamaBackend::load(&path).unwrap());
            let conversation = || {
                let mut tools = pr3_tool::builtin();
                tools.extend(pr3_tool::memory_tools(Arc::clone(&store)));
                Host::new(Arc::clone(&backend), tools)
                    .unwrap()
                    .with_memory(Arc::clone(&store))
            };

            let (_, events) = turn(
                &conversation(),
                "Please remember that my favourite colour is green.",
            );
            assert!(
                tool_names(&events).contains(&"remember"),
                "{id}: {events:?}"
            );

            // The host finds the memory and shows it to the model; the model may also
            // call recall itself.
            let (answer, _) = turn(&conversation(), "What is my favourite colour?");
            assert!(answer.to_lowercase().contains("green"), "{id}: {answer}");
            eprintln!("{id}: remembered ({answer})");
            let _ = std::fs::remove_file(&file);
            tested += 1;
        }
        assert!(
            tested > 0,
            "no selected model in {}",
            repo_models().display()
        );
    }
    /// A turn whose confirmation card is answered with `approve`; returns the reply and
    /// the events.
    fn turn_answering(
        host: &Host,
        actions: Arc<Actions>,
        text: &str,
        approve: bool,
    ) -> (String, Vec<TurnEvent>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let actions = tauri::async_runtime::block_on(host.send(text.to_string(), 0, move |e| {
            if let TurnEvent::Approval { request } = &e {
                actions.answer(&request.request, approve).unwrap();
            }
            sink.lock().unwrap().push(e);
        }))
        .unwrap();
        let reply = actions
            .iter()
            .find_map(|a| match a {
                Action::Reply { content, .. } => Some(content.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let events = events.lock().unwrap().clone();
        (reply, events)
    }

    /// The trusted path with each model: the model proposes an email to a contact by
    /// name, the user answers on the confirmation card, and only an approved email
    /// reaches the sandbox outbox.
    #[test]
    #[ignore = "needs the selected models in model/"]
    fn real_models_send_email_only_after_approval() {
        let config = Config::built_in().unwrap();
        let choice = &config.model;
        let mut tested = 0;
        for id in [&choice.desktop, &choice.mobile, &choice.mobile_small] {
            let path = repo_models().join(&config.models[id].file);
            if !path.is_file() {
                eprintln!("{id}: {} missing, skipped", path.display());
                continue;
            }
            let dir = std::env::temp_dir().join(format!("pr3-mail-{id}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let settings = &config.actions;
            let actions = Arc::new(
                Actions::with_vault(
                    &dir,
                    settings.limits(),
                    settings.directory(),
                    Box::new(SoftwareVault::generate().unwrap()),
                )
                .unwrap(),
            );
            let backend: Arc<dyn Backend> = Arc::new(LlamaBackend::load(&path).unwrap());
            let conversation = || {
                Host::new(Arc::clone(&backend), pr3_tool::builtin())
                    .unwrap()
                    .with_actions(Arc::clone(&actions))
                    .unwrap()
            };
            let outbox = || {
                std::fs::read_to_string(dir.join("outbox.jsonl"))
                    .map_or(0, |text| text.lines().count())
            };
            let asked = |events: &[TurnEvent]| {
                events.iter().find_map(|e| match e {
                    TurnEvent::Approval { request } => Some(request.mail.clone()),
                    _ => None,
                })
            };
            let sent = |events: &[TurnEvent]| {
                events.iter().find_map(|e| match e {
                    TurnEvent::Tool { name, success, .. } if name == "send_email" => Some(*success),
                    _ => None,
                })
            };

            let (_, events) = turn_answering(
                &conversation(),
                Arc::clone(&actions),
                "Email Alex to say I'll be ten minutes late.",
                true,
            );
            let mail =
                asked(&events).unwrap_or_else(|| panic!("{id}: no approval asked: {events:?}"));
            assert_eq!(mail.to[0].address, "alex@example.com", "{id}");
            assert_eq!(sent(&events), Some(true), "{id}: {events:?}");
            assert_eq!(outbox(), 1, "{id}");

            let (reply, events) = turn_answering(
                &conversation(),
                Arc::clone(&actions),
                "Send Sam Chen an email saying the meeting moved to 3pm.",
                false,
            );
            let mail =
                asked(&events).unwrap_or_else(|| panic!("{id}: no approval asked: {events:?}"));
            assert_eq!(mail.to[0].address, "sam@example.com", "{id}");
            assert_eq!(sent(&events), Some(false), "{id}: {events:?}");
            assert_eq!(outbox(), 1, "{id}: a declined email was delivered");
            // The reply must not claim the declined email went out.
            let said = reply.to_lowercase();
            let denies = ["not", "n't", "cancel", "won't"]
                .iter()
                .any(|w| said.contains(w));
            assert!(!said.contains("sent") || denies, "{id}: {reply}");
            eprintln!("{id}: approved one email, declined one; then said: {reply}");
            let _ = std::fs::remove_dir_all(&dir);
            tested += 1;
        }
        assert!(
            tested > 0,
            "no selected model in {}",
            repo_models().display()
        );
    }
}
