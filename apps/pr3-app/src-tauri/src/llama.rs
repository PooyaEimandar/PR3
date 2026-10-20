//! Host adapter: runs `pr3-core` inference requests on a bundled GGUF model.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use pr3_core::{InferenceRequest, Role};
use pr3_llamacpp::{ChatMessage, ChatRole, Engine, EngineConfig, EngineError};
use tauri::Manager;

use crate::host::Backend;

/// The development model fetched by `scripts/fetch-model.sh`.
pub const MODEL_FILE: &str = "Qwen3-0.6B-Q8_0.gguf";

const SYSTEM_PROMPT: &str = "You are PR3, a private assistant running entirely on the \
user's own device. Answer clearly and briefly.";

pub struct LlamaBackend {
    engine: Engine,
}

impl LlamaBackend {
    pub fn load(path: &Path) -> Result<Self, EngineError> {
        let config = EngineConfig {
            // All layers on the GPU: Metal on Apple silicon.
            gpu_layers: 999,
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

    fn generate(
        &self,
        request: &InferenceRequest,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<String, String> {
        let mut messages = Vec::with_capacity(request.messages.len() + 1);
        messages.push(ChatMessage {
            role: ChatRole::System,
            content: SYSTEM_PROMPT.to_string(),
        });
        messages.extend(request.messages.iter().map(|m| ChatMessage {
            role: match m.role {
                Role::System => ChatRole::System,
                Role::User => ChatRole::User,
                Role::Assistant => ChatRole::Assistant,
            },
            content: m.content.clone(),
        }));
        self.engine
            .chat(&messages, on_token)
            .map_err(|e| e.to_string())
    }
}

/// Where to look for the model, in order: `PR3_MODEL`, the app's data folder, and —
/// in debug builds only — the repository's `models/` folder.
pub fn model_candidates(app: &tauri::AppHandle) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) = std::env::var_os("PR3_MODEL") {
        paths.push(PathBuf::from(path));
    }
    if let Ok(dir) = app.path().app_data_dir() {
        paths.push(dir.join("models").join(MODEL_FILE));
    }
    if cfg!(debug_assertions) {
        paths.push(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../models")
                .join(MODEL_FILE),
        );
    }
    paths
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
    use crate::host::Host;

    /// Runs a real turn through core, host and Qwen. Opt-in because it needs the model:
    /// scripts/fetch-model.sh && cargo test -p pr3-app -- --ignored
    #[test]
    #[ignore = "needs models/Qwen3-0.6B-Q8_0.gguf"]
    fn real_model_turn_streams_and_replies() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../models")
            .join(MODEL_FILE);
        let host = Host::new(Arc::new(LlamaBackend::load(&path).unwrap()));

        let streamed = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&streamed);
        let actions = tauri::async_runtime::block_on(host.send(
            "What is the capital of France? Answer in one word.".to_string(),
            move |t| sink.lock().unwrap().push_str(t),
        ))
        .unwrap();

        let [Action::Reply { content, .. }] = actions.as_slice() else {
            panic!("expected one reply, got {actions:?}");
        };
        assert!(content.contains("Paris"), "unexpected answer: {content}");
        assert!(!content.contains("<think>"));
        assert_eq!(streamed.lock().unwrap().trim(), content.as_str());
    }
}
