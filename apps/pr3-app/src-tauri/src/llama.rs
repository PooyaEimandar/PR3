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

pub struct LlamaBackend {
    engine: Engine,
}

impl LlamaBackend {
    pub fn load(path: &Path) -> Result<Self, EngineError> {
        let config = EngineConfig {
            // Metal on Macs and iPhones. CPU on Android until a Vulkan build is measured
            // per device class (Roadmap §4.3), and in the iOS simulator, whose Metal
            // produces wrong results for llama.cpp's kernels.
            gpu_layers: if cfg!(any(target_os = "android", target_abi = "sim")) {
                0
            } else {
                999
            },
            seed: seed(),
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
        let mut messages = Vec::with_capacity(request.messages.len() + 1);
        let mut system = SYSTEM_PROMPT.to_string();
        if request.tools.iter().any(|t| t.name == "remember") {
            system.push_str(MEMORY_PROMPT);
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
            .chat_with_tools(&messages, &tools, on_token)
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

/// Where to look for the model `file`, in order: `PR3_MODEL`, the app's data folder,
/// and — in debug builds only — the repository's `models/` folder.
pub fn model_candidates(data_dir: Option<&Path>, file: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) = std::env::var_os("PR3_MODEL") {
        paths.push(PathBuf::from(path));
    }
    if let Some(dir) = data_dir {
        paths.push(dir.join("models").join(file));
    }
    #[cfg(debug_assertions)]
    paths.push(repo_models().join(file));
    paths
}

/// The repository's `models/` folder. Debug builds and tests only, so release builds
/// carry no path from the build machine.
#[cfg(any(debug_assertions, test))]
fn repo_models() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../models")
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
    use crate::config::Config;
    use crate::host::{Backend, Host, TurnEvent};

    fn host(path: &Path) -> Host {
        Host::new(
            Arc::new(LlamaBackend::load(path).unwrap()),
            pr3_tools::builtin(),
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
                TurnEvent::Token { .. } => None,
            })
            .collect()
    }

    /// Runs real turns through core, host, Qwen and the built-in tools, for each model
    /// config/pr3.toml selects (desktop, mobile, small mobile) that is in models/.
    /// Opt-in because it needs them:
    /// scripts/fetch-model.sh && cargo test -p pr3-app --release -- --ignored --nocapture
    #[test]
    #[ignore = "needs the selected models in models/"]
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
    #[ignore = "needs the selected models in models/"]
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
            let store: pr3_tools::SharedMemory = Arc::new(Mutex::new(
                pr3_memory_sqlite::SqliteMemory::open(&file).unwrap(),
            ));
            let backend: Arc<dyn Backend> = Arc::new(LlamaBackend::load(&path).unwrap());
            let conversation = || {
                let mut tools = pr3_tools::builtin();
                tools.extend(pr3_tools::memory_tools(Arc::clone(&store)));
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
}
