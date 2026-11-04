//! The core's agent loop under arbitrary model replies, tool calls and tool results.
//! It never panics, runs only registered tools with arguments that fit their schema,
//! and never runs a consequential tool directly: those reach the host only as
//! proposals, for the trusted confirmation path, and never once the turn has read
//! untrusted text (docs RFC-002).

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
    arguments: Arguments,
}

/// A call's arguments: any text, or a tool's own shape with any values, so that rounds
/// with valid calls to both tools, as a read beside a send, are common.
#[derive(Arbitrary, Debug)]
enum Arguments {
    Text(String),
    Lookup { query: String, limit: Option<i64> },
    Send { to: String, body: String },
}

impl Arguments {
    fn text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Lookup { query, limit } => {
                let mut arguments = Map::new();
                arguments.insert("query".to_string(), Value::from(query.as_str()));
                if let Some(limit) = limit {
                    arguments.insert("limit".to_string(), Value::from(*limit));
                }
                Value::Object(arguments).to_string()
            }
            Self::Send { to, body } => {
                let mut arguments = Map::new();
                arguments.insert("to".to_string(), Value::from(to.as_str()));
                arguments.insert("body".to_string(), Value::from(body.as_str()));
                Value::Object(arguments).to_string()
            }
        }
    }
}

const LOOKUP: &str = "lookup";
const SEND: &str = "send_mail";

fn tools() -> [ToolSpec; 2] {
    // The plain tool's results are untrusted, like a web page's.
    let spec = |name: &str, parameters: &str, consequential: bool| ToolSpec {
        name: name.to_string(),
        description: String::new(),
        parameters: parameters.to_string(),
        requires_confirmation: consequential,
        untrusted_output: !consequential,
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
    // Whether the turn has had a successful lookup, whose result is untrusted.
    let mut read = false;
    for round in turn.rounds {
        let mut next = Vec::new();
        let mut outcomes = round.outcomes.iter();
        for action in pending {
            check(&action);
            clock += 1;
            let at = Timestamp(clock);
            let mut reads = false;
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
                            arguments: c.arguments.text(),
                        })
                        .collect(),
                    info: GenerationInfo::default(),
                    at,
                },
                Action::CallTool(call) | Action::Propose(call) => {
                    let (success, output) =
                        outcomes.next().cloned().unwrap_or((true, String::new()));
                    reads = success && call.name == LOOKUP;
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
                let proposes = actions.iter().any(|a| matches!(a, Action::Propose(_)));
                let reading = actions
                    .iter()
                    .any(|a| matches!(a, Action::CallTool(c) if c.name == LOOKUP));
                assert!(
                    !read || !proposes,
                    "a consequential call was proposed after an untrusted read"
                );
                assert!(
                    !(reading && proposes),
                    "a consequential call was proposed beside an untrusted read"
                );
                next.extend(actions);
            }
            read |= reads;
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
