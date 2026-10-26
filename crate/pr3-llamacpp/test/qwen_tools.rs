//! Real-model check of the tool formats, run against every supported model found in
//! model/. Opt-in because it needs them:
//! script/fetch-model.sh && cargo test -p pr3-llamacpp --release -- --ignored --nocapture

use std::path::Path;

use pr3_llamacpp::{ChatMessage, ChatRole, Engine, EngineConfig, PromptFormat, ToolDef};

const MODELS: [&str; 4] = [
    "Qwen3-0.6B-Q8_0.gguf",
    "Qwen3.5-2B-Q4_K_M.gguf",
    "Qwen3.5-4B-Q4_K_M.gguf",
    "Qwen3.8-27B-Q4_K_M.gguf",
];

fn calculator() -> ToolDef {
    ToolDef {
        name: "calculate".to_string(),
        description: "Evaluate an arithmetic expression exactly.".to_string(),
        parameters: r#"{"type":"object","properties":{"expression":{"type":"string","description":"For example 12*(3+4)"}},"required":["expression"]}"#.to_string(),
    }
}

#[test]
#[ignore = "needs the models in model/"]
fn each_model_calls_a_tool_in_its_own_format() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models");
    let present: Vec<_> = MODELS
        .iter()
        .map(|file| dir.join(file))
        .filter(|path| path.is_file())
        .collect();
    assert!(!present.is_empty(), "no models in {}", dir.display());

    for path in present {
        let config = EngineConfig {
            gpu_layers: if cfg!(target_os = "macos") { 999 } else { 0 },
            ..EngineConfig::default()
        };
        let engine = Engine::load(&path, config).expect("model loads");
        let expected = if engine.label().starts_with("Qwen3-") {
            PromptFormat::Qwen3
        } else {
            PromptFormat::Qwen35 {
                preserve_thinking: engine.label().starts_with("Qwen3.8"),
            }
        };
        assert_eq!(engine.format(), expected, "{}", engine.label());

        let messages = [ChatMessage::new(
            ChatRole::User,
            "What is 1234 * 5678? Use the calculator.",
        )];
        let mut streamed = String::new();
        let out = engine
            .chat_with_tools(&messages, &[calculator()], &mut |t| streamed.push_str(t))
            .expect("generates");

        eprintln!(
            "{}: text {:?}, calls {:?}",
            engine.label(),
            out.text,
            out.tool_calls
        );
        assert_eq!(out.tool_calls.len(), 1, "{}: {out:?}", engine.label());
        assert_eq!(out.tool_calls[0].name, "calculate");
        let arguments: serde_json::Value =
            serde_json::from_str(&out.tool_calls[0].arguments).expect("JSON arguments");
        let expression = arguments["expression"].as_str().unwrap_or_default();
        assert!(expression.contains("1234"), "{}: {out:?}", engine.label());
        assert!(
            !streamed.contains("<tool_call>") && !streamed.contains("<function="),
            "calls are hidden from the stream"
        );
        assert!(out.prompt_tokens > 0 && out.completion_tokens > 0);
    }
}
