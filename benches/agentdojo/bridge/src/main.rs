//! The Rust half of PR3's AgentDojo harness (docs Sec. 8.1, spike step 8). Benchmark only:
//! never part of a shipped build.
//!
//! AgentDojo's tasks, tools and scoring live in Python (`../pr3_agentdojo`); the model
//! and the agent loops live here, so every arm runs the same weights through the same
//! engine and prompt code and only the agent loop differs:
//!
//! - `pr3`: PR3's core. Calls are validated against the tool's schema, loops are
//!   bounded, and calls to consequential tools come out as `Action::Propose`: they run
//!   only if the user approves the exact call.
//! - `plain`: a plain tool-calling loop. Every call the model makes runs.
//! - `confirm_all`: the plain loop, asking the same approver before every consequential
//!   call. A control: it separates PR3's runtime from simply asking the user.
//!
//! The two processes talk JSON Lines over stdin and stdout; this side logs to stderr.

use std::io::{self, BufRead as _, Write as _};
use std::path::PathBuf;
use std::time::Instant;

use pr3_core::{
    Action, Config, Core, GenerationInfo, InferenceRequest, Input, ProposedToolCall, Role,
    SessionId, Timestamp, ToolSpec,
};
use pr3_llamacpp::{
    ChatMessage, ChatOutput, ChatRole, Engine, EngineConfig, ToolCallText, ToolDef,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Pr3,
    Plain,
    ConfirmAll,
}

#[derive(Debug, Deserialize)]
struct Tool {
    name: String,
    description: String,
    /// JSON Schema, already reduced to the subset PR3 checks.
    parameters: Value,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum FromDriver {
    Run {
        mode: Mode,
        system: String,
        prompt: String,
        tools: Vec<Tool>,
        /// Tools that change something or send data out: they need approval in `pr3`
        /// and `confirm_all`.
        consequential: Vec<String>,
        max_rounds: u32,
    },
    ToolResult {
        id: String,
        output: String,
        error: Option<String>,
    },
    Approval {
        id: String,
        approve: bool,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ToDriver {
    Ready {
        model: String,
    },
    /// Run this call against the environment.
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
    /// Ask the approver about this exact call before it runs.
    Approve {
        id: String,
        name: String,
        arguments: Value,
    },
    Done {
        reply: String,
        /// Why the run ended without an answer, if it did.
        failed: Option<String>,
        stats: Stats,
    },
}

#[derive(Debug, Default, Serialize)]
struct Stats {
    model_calls: u32,
    prompt_tokens: u64,
    completion_tokens: u64,
    seconds: f64,
    /// Calls the model proposed in all.
    proposed: u32,
    /// Calls that ran against the environment.
    executed: u32,
    /// Calls refused before anyone was asked: malformed, unknown tool, over a limit.
    refused: u32,
    approvals_asked: u32,
    declined: u32,
}

struct Driver {
    stdin: io::StdinLock<'static>,
    stdout: io::StdoutLock<'static>,
}

impl Driver {
    fn send(&mut self, message: &ToDriver) -> io::Result<()> {
        let line = serde_json::to_string(message).map_err(io::Error::other)?;
        writeln!(self.stdout, "{line}")?;
        self.stdout.flush()
    }

    fn recv(&mut self) -> io::Result<Option<FromDriver>> {
        let mut line = String::new();
        if self.stdin.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        serde_json::from_str(&line)
            .map(Some)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Runs a call against the environment; returns its output for the model.
    fn execute(&mut self, id: &str, name: &str, arguments: Value) -> io::Result<(bool, String)> {
        self.send(&ToDriver::ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments,
        })?;
        match self.recv()? {
            Some(FromDriver::ToolResult {
                id: got,
                output,
                error,
            }) if got == id => Ok(match error {
                Some(error) => (false, error),
                None => (true, output),
            }),
            other => Err(io::Error::other(format!(
                "expected a tool result, got {other:?}"
            ))),
        }
    }

    fn approve(&mut self, id: &str, name: &str, arguments: Value) -> io::Result<bool> {
        self.send(&ToDriver::Approve {
            id: id.to_string(),
            name: name.to_string(),
            arguments,
        })?;
        match self.recv()? {
            Some(FromDriver::Approval { id: got, approve }) if got == id => Ok(approve),
            other => Err(io::Error::other(format!(
                "expected an approval, got {other:?}"
            ))),
        }
    }
}

/// What the model is told when its call is declined, the same in every arm.
const DECLINED: &str = "NOT DONE. The user declined this action on the confirmation screen.";

fn main() -> io::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("usage: pr3-bench-agentdojo MODEL.gguf"))?;
    let seed = std::env::var("PR3_BENCH_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);
    let config = EngineConfig {
        n_ctx: 16384,
        max_new_tokens: 1024,
        gpu_layers: if cfg!(target_os = "macos") { 999 } else { 0 },
        seed,
        ..EngineConfig::default()
    };
    let engine = Engine::load(&path, config).map_err(|e| io::Error::other(e.to_string()))?;
    let mut driver = Driver {
        stdin: io::stdin().lock(),
        stdout: io::stdout().lock(),
    };
    driver.send(&ToDriver::Ready {
        model: engine.label().to_string(),
    })?;
    while let Some(message) = driver.recv()? {
        let FromDriver::Run {
            mode,
            system,
            prompt,
            tools,
            consequential,
            max_rounds,
        } = message
        else {
            return Err(io::Error::other("expected a run"));
        };
        let started = Instant::now();
        let mut stats = Stats::default();
        let result = match mode {
            Mode::Pr3 => run_pr3(
                &engine,
                &mut driver,
                &system,
                &prompt,
                &tools,
                &consequential,
                max_rounds,
                &mut stats,
            ),
            Mode::Plain | Mode::ConfirmAll => run_plain(
                &engine,
                &mut driver,
                &system,
                &prompt,
                &tools,
                (mode == Mode::ConfirmAll).then_some(consequential.as_slice()),
                max_rounds,
                &mut stats,
            ),
        }?;
        stats.seconds = started.elapsed().as_secs_f64();
        let (reply, failed) = match result {
            Ok(reply) => (reply, None),
            Err(reason) => (String::new(), Some(reason)),
        };
        driver.send(&ToDriver::Done {
            reply,
            failed,
            stats,
        })?;
    }
    Ok(())
}

fn tool_defs(tools: &[Tool]) -> Vec<ToolDef> {
    tools
        .iter()
        .map(|t| ToolDef {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters: t.parameters.to_string(),
        })
        .collect()
}

fn generate(
    engine: &Engine,
    messages: &[ChatMessage],
    tools: &[ToolDef],
    stats: &mut Stats,
) -> Result<ChatOutput, String> {
    let out = engine
        .chat_with_tools(messages, tools, &mut |_| {})
        .map_err(|e| e.to_string())?;
    stats.model_calls += 1;
    stats.prompt_tokens += u64::from(out.prompt_tokens);
    stats.completion_tokens += u64::from(out.completion_tokens);
    Ok(out)
}

/// PR3's core drives the turn; this host performs its actions.
#[allow(clippy::too_many_arguments)]
fn run_pr3(
    engine: &Engine,
    driver: &mut Driver,
    system: &str,
    prompt: &str,
    tools: &[Tool],
    consequential: &[String],
    max_rounds: u32,
    stats: &mut Stats,
) -> io::Result<Result<String, String>> {
    let mut core = Core::new(Config {
        max_tool_rounds: max_rounds,
        max_tool_calls_per_turn: 48,
        max_input_bytes: 64 * 1024,
        // The plain loop never drops history; neither does PR3 here.
        max_history: 512,
        ..Config::default()
    });
    for tool in tools {
        let spec = ToolSpec {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters: tool.parameters.to_string(),
            requires_confirmation: consequential.contains(&tool.name),
            required_capabilities: Vec::new(),
        };
        if let Err(e) = core.register_tool(spec) {
            return Ok(Err(format!("tool {} not accepted: {e}", tool.name)));
        }
    }
    let defs = tool_defs(tools);
    let session = SessionId(1);
    let mut clock = 0u64;
    let mut now = || {
        clock += 1;
        Timestamp(clock)
    };
    let step = |core: &mut Core, input: Input| core.step(input).map_err(|e| e.to_string());
    let mut queue = match step(
        &mut core,
        Input::Utterance {
            session,
            content: prompt.to_string(),
            notes: Vec::new(),
            at: now(),
        },
    ) {
        Ok(actions) => actions,
        Err(e) => return Ok(Err(e)),
    };
    while !queue.is_empty() {
        let action = queue.remove(0);
        let next = match action {
            Action::Infer(request) => {
                let messages = chat_messages(system, &request);
                let input = match generate(engine, &messages, &defs, stats) {
                    Ok(out) => {
                        stats.proposed += u32::try_from(out.tool_calls.len()).unwrap_or(u32::MAX);
                        Input::ModelOutput {
                            request: request.id,
                            content: out.text,
                            tool_calls: out
                                .tool_calls
                                .into_iter()
                                .map(|c| ProposedToolCall {
                                    name: c.name,
                                    arguments: c.arguments,
                                })
                                .collect(),
                            info: GenerationInfo::default(),
                            at: now(),
                        }
                    }
                    Err(reason) => Input::ModelFailed {
                        request: request.id,
                        reason,
                        at: now(),
                    },
                };
                step(&mut core, input)
            }
            Action::CallTool(call) => {
                let arguments: Value = serde_json::from_str(&call.arguments).unwrap_or(Value::Null);
                let (success, output) = driver.execute(&call.id, &call.name, arguments)?;
                stats.executed += 1;
                step(
                    &mut core,
                    Input::ToolOutcome {
                        call_id: call.id,
                        success,
                        output,
                        at: now(),
                    },
                )
            }
            Action::Propose(call) => {
                // The trusted path, reduced to what AgentDojo can exercise: the user sees
                // the exact, schema-checked call, and only that call runs.
                let arguments: Value = serde_json::from_str(&call.arguments).unwrap_or(Value::Null);
                stats.approvals_asked += 1;
                let (success, output) =
                    if driver.approve(&call.id, &call.name, arguments.clone())? {
                        stats.executed += 1;
                        driver.execute(&call.id, &call.name, arguments)?
                    } else {
                        stats.declined += 1;
                        (false, DECLINED.to_string())
                    };
                step(
                    &mut core,
                    Input::ToolOutcome {
                        call_id: call.id,
                        success,
                        output,
                        at: now(),
                    },
                )
            }
            Action::Reply { content, .. } => {
                stats.refused = stats
                    .proposed
                    .saturating_sub(stats.executed + stats.declined);
                return Ok(Ok(content));
            }
            Action::Failed { reason, .. } => {
                stats.refused = stats
                    .proposed
                    .saturating_sub(stats.executed + stats.declined);
                return Ok(Err(reason));
            }
            Action::Trace(_) => Ok(Vec::new()),
        };
        match next {
            Ok(actions) => queue.extend(actions),
            Err(e) => return Ok(Err(e)),
        }
    }
    Ok(Err("the turn ended without an answer".to_string()))
}

/// The conversation the core asks about, as the engine takes it.
fn chat_messages(system: &str, request: &InferenceRequest) -> Vec<ChatMessage> {
    let mut messages = vec![ChatMessage::new(ChatRole::System, system)];
    messages.extend(request.messages.iter().map(|m| {
        let role = match m.role {
            Role::System => ChatRole::System,
            Role::User => ChatRole::User,
            Role::Assistant => ChatRole::Assistant,
            Role::Tool => ChatRole::Tool,
        };
        ChatMessage {
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
    messages
}

/// A plain tool-calling loop: every call the model makes runs, unless `confirm` names
/// its tool, in which case the approver is asked first.
#[allow(clippy::too_many_arguments)]
fn run_plain(
    engine: &Engine,
    driver: &mut Driver,
    system: &str,
    prompt: &str,
    tools: &[Tool],
    confirm: Option<&[String]>,
    max_rounds: u32,
    stats: &mut Stats,
) -> io::Result<Result<String, String>> {
    let defs = tool_defs(tools);
    let mut messages = vec![
        ChatMessage::new(ChatRole::System, system),
        ChatMessage::new(ChatRole::User, prompt),
    ];
    let mut next_id = 0u32;
    for _ in 0..=max_rounds {
        let out = match generate(engine, &messages, &defs, stats) {
            Ok(out) => out,
            Err(reason) => return Ok(Err(reason)),
        };
        if out.tool_calls.is_empty() {
            return Ok(Ok(out.text));
        }
        stats.proposed += u32::try_from(out.tool_calls.len()).unwrap_or(u32::MAX);
        messages.push(ChatMessage {
            tool_calls: out.tool_calls.clone(),
            ..ChatMessage::new(ChatRole::Assistant, out.text.clone())
        });
        for call in out.tool_calls {
            next_id += 1;
            let id = format!("call-{next_id}");
            let output = match serde_json::from_str::<Value>(&call.arguments) {
                Err(_) => {
                    stats.refused += 1;
                    "error: arguments are not valid JSON".to_string()
                }
                Ok(arguments) => {
                    let ask = confirm.is_some_and(|c| c.contains(&call.name));
                    if ask {
                        stats.approvals_asked += 1;
                    }
                    if ask && !driver.approve(&id, &call.name, arguments.clone())? {
                        stats.declined += 1;
                        DECLINED.to_string()
                    } else {
                        stats.executed += 1;
                        match driver.execute(&id, &call.name, arguments)? {
                            (true, output) => output,
                            (false, error) => format!("error: {error}"),
                        }
                    }
                }
            };
            messages.push(ChatMessage::new(ChatRole::Tool, output));
        }
    }
    Ok(Err(format!(
        "stopped after {max_rounds} rounds of tool calls"
    )))
}
