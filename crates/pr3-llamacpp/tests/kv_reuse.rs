//! Real-model check that a conversation stays correct, and gets faster, as the engine
//! reuses what earlier calls already processed: a tool round within one turn, then a
//! follow-up turn that needs the start of the conversation. Prints each call's timing.
//! Opt-in because it needs the models:
//! scripts/fetch-model.sh && cargo test -p pr3-llamacpp --release --test kv_reuse -- --ignored --nocapture
//! PR3_TEST_GPU_LAYERS=0 runs fully on the CPU, as phones and simulators do;
//! PR3_TEST_MODEL=Qwen3.5-4B picks one model.

use std::path::Path;
use std::time::Instant;

use pr3_llamacpp::{ChatMessage, ChatOutput, ChatRole, Engine, EngineConfig, ToolDef};

const MODELS: [&str; 3] = [
    "Qwen3-0.6B-Q8_0.gguf",
    "Qwen3.5-4B-Q4_K_M.gguf",
    "Qwen3.8-27B-Q4_K_M.gguf",
];

fn calculator() -> ToolDef {
    ToolDef {
        name: "calculate".to_string(),
        description: "Evaluate an arithmetic expression exactly.".to_string(),
        parameters: r#"{"type":"object","properties":{"expression":{"type":"string"}},"required":["expression"]}"#.to_string(),
    }
}

fn call(engine: &Engine, messages: &[ChatMessage], step: &str) -> ChatOutput {
    let started = Instant::now();
    let out = engine
        .chat_with_tools(messages, &[calculator()], &mut |_| {})
        .expect("generates");
    eprintln!(
        "{} {step}: {:.2?}, prompt {} tokens, {} reused",
        engine.label(),
        started.elapsed(),
        out.prompt_tokens,
        out.reused_tokens,
    );
    out
}

#[test]
#[ignore = "needs the models in models/"]
fn conversation_stays_correct_while_reusing_the_cache() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models");
    let gpu_layers = std::env::var("PR3_TEST_GPU_LAYERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(if cfg!(target_os = "macos") { 999 } else { 0 });
    let present: Vec<_> = MODELS
        .iter()
        .map(|file| dir.join(file))
        .filter(|path| path.is_file())
        .filter(|path| {
            std::env::var("PR3_TEST_MODEL")
                .map_or(true, |only| path.to_string_lossy().contains(&only))
        })
        .collect();
    assert!(!present.is_empty(), "no models in {}", dir.display());

    for path in present {
        let config = EngineConfig {
            gpu_layers,
            ..EngineConfig::default()
        };
        let engine = Engine::load(&path, config).expect("model loads");
        let mut messages = vec![
            ChatMessage::new(ChatRole::System, "You are PR3. Answer briefly."),
            ChatMessage::new(
                ChatRole::User,
                "My name is Ada. What is 1234 * 5678? Use the calculator.",
            ),
        ];

        // Turn 1, call 1: the model asks for the calculator.
        let out = call(&engine, &messages, "tool call");
        assert_eq!(out.tool_calls.len(), 1, "{}: {out:?}", engine.label());
        messages.push(ChatMessage {
            tool_calls: out.tool_calls.clone(),
            ..ChatMessage::new(ChatRole::Assistant, out.text)
        });
        messages.push(ChatMessage::new(ChatRole::Tool, "7006652"));

        // Turn 1, call 2: the answer, after the tool result. Everything up to the tool
        // result was processed by call 1.
        let out = call(&engine, &messages, "answer");
        assert!(
            out.reused_tokens > out.prompt_tokens / 2,
            "{}: {out:?}",
            engine.label()
        );
        assert!(
            out.text.replace(',', "").contains("7006652"),
            "{}: {out:?}",
            engine.label()
        );
        messages.push(ChatMessage::new(ChatRole::Assistant, out.text));

        // Turn 2: needs the start of the conversation.
        messages.push(ChatMessage::new(ChatRole::User, "What is my name?"));
        let out = call(&engine, &messages, "next turn");
        assert!(
            out.reused_tokens > out.prompt_tokens / 2,
            "{}: {out:?}",
            engine.label()
        );
        assert!(out.text.contains("Ada"), "{}: {out:?}", engine.label());
        assert!(out.tool_calls.is_empty(), "{}: {out:?}", engine.label());
    }
}
