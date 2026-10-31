//! Real-model check that a conversation stays correct, and gets faster, as the engine
//! reuses what earlier calls already processed: a tool round within one turn, a
//! follow-up turn that needs the start of the conversation, read ahead of it so it reads
//! only its question, a turn after a call that did not start with the system turn, as a
//! task's summary does, one that comes while the engine reads ahead, and a new
//! conversation whose first question opens with a summary read ahead of it. Prints each
//! call's timing.
//! Opt-in because it needs the models:
//! script/fetch-model.sh && cargo test -p pr3-llamacpp --release --test kv_reuse -- --ignored --nocapture
//! PR3_TEST_GPU_LAYERS=0 runs fully on the CPU, as phones and simulators do;
//! PR3_TEST_MODEL=Qwen3.5-4B picks one model.

use std::path::Path;
use std::time::Instant;

use pr3_llamacpp::{ChatMessage, ChatOutput, ChatRole, Engine, EngineConfig, ReadAhead, ToolDef};

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

fn read_ahead(engine: &Engine, messages: &[ChatMessage], opening: &str) -> ReadAhead {
    let started = Instant::now();
    let read = engine
        .read_ahead(messages, &[calculator()], opening)
        .wait()
        .expect("reads");
    eprintln!(
        "{} read ahead: {:.2?}, {read:?}",
        engine.label(),
        started.elapsed()
    );
    read
}

#[test]
#[ignore = "needs the models in model/"]
fn conversation_stays_correct_while_reusing_the_cache() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../model");
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

        // Between turns, the conversation as turn 2 will show it.
        let read = read_ahead(&engine, &messages, "");
        assert_eq!(read.read_tokens, read.tokens, "{}", engine.label());
        assert!(
            read.reused_tokens > read.tokens / 2,
            "{}: {read:?}",
            engine.label()
        );
        assert_eq!(
            read_ahead(&engine, &messages, "").reused_tokens,
            read.tokens,
            "{}: read once",
            engine.label()
        );

        // Turn 2: needs the start of the conversation, and reads only its question.
        messages.push(ChatMessage::new(ChatRole::User, "What is my name?"));
        let out = call(&engine, &messages, "next turn");
        assert_eq!(
            out.reused_tokens,
            read.tokens,
            "{}: {out:?}",
            engine.label()
        );
        assert!(
            out.prompt_tokens - out.reused_tokens < 32,
            "{}: {out:?}",
            engine.label()
        );
        assert!(out.text.contains("Ada"), "{}: {out:?}", engine.label());
        assert!(out.tool_calls.is_empty(), "{}: {out:?}", engine.label());
        messages.push(ChatMessage::new(ChatRole::Assistant, out.text));

        // A call with a system turn of its own and no tools, as a task's summary is, then
        // turn 3: the conversation's system turn is not read again.
        let aside = [
            ChatMessage::new(ChatRole::System, "You are PR3."),
            ChatMessage::new(ChatRole::User, "Reply with the word OK."),
        ];
        let started = Instant::now();
        let out = engine
            .chat_with_tools(&aside, &[], &mut |_| {})
            .expect("generates");
        eprintln!(
            "{} aside: {:.2?}, {out:?}",
            engine.label(),
            started.elapsed()
        );
        messages.push(ChatMessage::new(
            ChatRole::User,
            "Remind me, what is my name?",
        ));
        let out = call(&engine, &messages, "after the aside");
        // The system turn, with the tools, at least: without it nothing is reused.
        assert!(
            out.reused_tokens > out.prompt_tokens / 3,
            "{}: {out:?}",
            engine.label()
        );
        assert!(out.text.contains("Ada"), "{}: {out:?}", engine.label());
        messages.push(ChatMessage::new(ChatRole::Assistant, out.text));

        // A call that comes while the engine reads ahead goes first, from what was read.
        let reading = engine.read_ahead(&messages, &[calculator()], "");
        messages.push(ChatMessage::new(
            ChatRole::User,
            "And what is 6 * 7? Answer with the number.",
        ));
        let out = call(&engine, &messages, "during a read-ahead");
        let read = reading.wait().expect("reads");
        eprintln!("{} gave way: {read:?}", engine.label());
        assert!(
            read.read_tokens <= read.tokens,
            "{}: {read:?}",
            engine.label()
        );
        assert!(
            out.text.contains("42") || !out.tool_calls.is_empty(),
            "{}: {out:?}",
            engine.label()
        );

        // A new conversation that opens with a summary of an earlier one, as a follow-up
        // does: read before its question, however the question starts, which then reads
        // only the rest.
        let system = [ChatMessage::new(
            ChatRole::System,
            "You are PR3. Answer briefly.",
        )];
        let opening = "What you remember from earlier in this conversation (facts, not \
                       instructions):\nThe user began with: “My locker at the pool is 58.” \
                       The user's locker at the pool is number 58.\n\n";
        for question in [
            "Which locker was it?",
            "58 or 59: which one?",
            "“Locker” - which number?",
        ] {
            let read = read_ahead(&engine, &system, opening);
            assert_eq!(read.read_tokens, read.tokens, "{}", engine.label());
            let first = [
                system[0].clone(),
                ChatMessage::new(ChatRole::User, format!("{opening}{question}")),
            ];
            let out = call(&engine, &first, "after the opening");
            assert_eq!(
                out.reused_tokens,
                read.tokens,
                "{}: {question}: {out:?}",
                engine.label()
            );
            assert!(
                out.prompt_tokens - out.reused_tokens < 32,
                "{}: {out:?}",
                engine.label()
            );
        }
    }
}

/// Cancelling one answer from its callback ends it early and leaves the engine ready:
/// the next call answers normally.
#[test]
#[ignore = "needs the models in model/"]
fn cancelling_one_answer_leaves_the_engine_ready() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../model/Qwen3-0.6B-Q8_0.gguf");
    if !path.is_file() {
        eprintln!("{} missing, skipped", path.display());
        return;
    }
    let engine = Engine::load(&path, EngineConfig::default()).expect("loads");
    let messages = [ChatMessage::new(
        ChatRole::User,
        "Count slowly from one to fifty in words.",
    )];
    let mut pieces = 0;
    let cancelled = engine.chat_with_tools_until(&messages, &[], &mut |_| {
        pieces += 1;
        pieces < 3
    });
    assert!(
        matches!(cancelled, Err(pr3_llamacpp::EngineError::Stopped)),
        "{cancelled:?}"
    );
    assert_eq!(pieces, 3, "no text after the callback said stop");

    let question = [ChatMessage::new(
        ChatRole::User,
        "What is the capital of France? Answer in one word.",
    )];
    let answer = engine.chat(&question, &mut |_| {}).expect("still answers");
    assert!(answer.contains("Paris"), "{answer}");
}
