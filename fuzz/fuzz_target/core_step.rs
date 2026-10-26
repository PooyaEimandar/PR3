//! The core's agent loop under arbitrary model replies, tool calls and tool results.
//! It never panics, runs only registered tools with arguments that fit their schema,
//! and never runs a consequential tool directly: those reach the host only as
//! proposals, for the trusted confirmation path.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use pr3_core::{
    Action, Config, Core, GenerationInfo, Input, ProposedToolCall, SessionId, Timestamp, ToolSpec,
};
use serde_json::{Map, Value};

#[derive(Arbitrary, Debug)]
struct Turn {
    utterance: String,
    notes: Vec<String>,
    rounds: Vec<Round>,
}

#[derive(Arbitrary, Debug)]
struct Round {
    content: String,
    calls: Vec<Call>,
    outcomes: Vec<(bool, String)>,
    model_fails: bool,
}

#[derive(Arbitrary, Debug)]
struct Call {
    /// Which tool: one of the two registered ones, or `name` itself.
    tool: u8,
    name: String,
    arguments: String,
}

const LOOKUP: &str = "lookup";
const SEND: &str = "send_mail";

fn tools() -> [ToolSpec; 2] {
    let spec = |name: &str, parameters: &str, consequential: bool| ToolSpec {
        name: name.to_string(),
        description: String::new(),
        parameters: parameters.to_string(),
        requires_confirmation: consequential,
        required_capabilities: Vec::new(),
    };
    [
        spec(
            LOOKUP,
            r#"{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"}},"required":["query"]}"#,
            false,
        ),
        spec(
            SEND,
            r#"{"type":"object","properties":{"to":{"type":"string"},"body":{"type":"string"}},"required":["to","body"]}"#,
            true,
        ),
    ]
}

fuzz_target!(|turn: Turn| {
    let mut core = Core::new(Config {
        max_input_bytes: 4096,
        max_tool_args_bytes: 512,
        max_tool_output_bytes: 512,
        ..Config::default()
    });
    for spec in tools() {
        core.register_tool(spec).unwrap();
    }
    let session = SessionId(1);
    let mut clock = 1u64;
    let utterance = Input::Utterance {
        session,
        content: turn.utterance,
        notes: turn.notes,
        at: Timestamp(clock),
    };
    let Ok(mut pending) = core.step(utterance) else {
        return;
    };
    for round in turn.rounds {
        let mut next = Vec::new();
        let mut outcomes = round.outcomes.iter();
        for action in pending {
            check(&action);
            clock += 1;
            let at = Timestamp(clock);
            let input = match action {
                Action::Infer(request) if round.model_fails => Input::ModelFailed {
                    request: request.id,
                    reason: "the model went away".to_string(),
                    at,
                },
                Action::Infer(request) => Input::ModelOutput {
                    request: request.id,
                    content: round.content.clone(),
                    tool_calls: round
                        .calls
                        .iter()
                        .map(|c| ProposedToolCall {
                            name: match c.tool % 3 {
                                0 => LOOKUP.to_string(),
                                1 => SEND.to_string(),
                                _ => c.name.clone(),
                            },
                            arguments: c.arguments.clone(),
                        })
                        .collect(),
                    info: GenerationInfo::default(),
                    at,
                },
                Action::CallTool(call) | Action::Propose(call) => {
                    let (success, output) =
                        outcomes.next().cloned().unwrap_or((true, String::new()));
                    Input::ToolOutcome {
                        call_id: call.id,
                        success,
                        output,
                        at,
                    }
                }
                _ => continue,
            };
            if let Ok(actions) = core.step(input) {
                next.extend(actions);
            }
        }
        if next.is_empty() {
            return;
        }
        pending = next;
    }
    for action in &pending {
        check(action);
    }
});

/// What the core lets through: the right path for each tool, and arguments that fit
/// the tool's schema exactly.
fn check(action: &Action) {
    let (call, allowed): (_, &[(&str, bool)]) = match action {
        Action::CallTool(call) => {
            assert_eq!(call.name, LOOKUP, "only the plain tool runs directly");
            (call, &[("query", true), ("limit", false)])
        }
        Action::Propose(call) => {
            assert_eq!(call.name, SEND, "only the consequential tool is proposed");
            (call, &[("to", true), ("body", true)])
        }
        _ => return,
    };
    let arguments: Map<String, Value> =
        serde_json::from_str(&call.arguments).expect("validated arguments are a JSON object");
    for key in arguments.keys() {
        assert!(
            allowed.iter().any(|(k, _)| k == key),
            "unexpected argument {key}"
        );
    }
    for (key, required) in allowed {
        match arguments.get(*key) {
            Some(Value::String(_)) => {}
            Some(Value::Number(n)) if *key == "limit" => assert!(n.is_i64() || n.is_u64()),
            None => assert!(!required, "missing required argument {key}"),
            Some(other) => panic!("argument {key} has the wrong type: {other}"),
        }
    }
}
