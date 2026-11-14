//! Benchmark only: the real PR3 Host and upstream OpenJarvis share the exact Rust
//! inference engine and document tools over a local stdio boundary. No production
//! HTTP provider, network access, Python dependency or extra action authority.
use pr3_app_lib::bench::{Host, LlamaBackend, TurnEvent, block_on};
use pr3_llamacpp::{ChatMessage, ChatRole, Engine, EngineConfig};
use pr3_tool::{Tool, ToolContext};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Write},
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};
struct Run {
    host: Option<Host>,
    engine: Option<Engine>,
    tools: Vec<Box<dyn Tool>>,
    calls: usize,
    rounds: usize,
}
fn string<'a>(v: &'a Value, k: &str) -> Result<&'a str, String> {
    v.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{k} is required"))
}
fn init(v: &Value) -> Result<Run, String> {
    let mut db = pr3_memory_sqlite::SqliteMemory::open_in_memory().map_err(|e| e.to_string())?;
    for d in v
        .get("documents")
        .and_then(Value::as_array)
        .ok_or("documents required")?
    {
        db.import_document(string(d, "name")?, string(d, "text")?)
            .map_err(|e| e.to_string())?;
    }
    let library = Arc::new(Mutex::new(db));
    let model = Path::new(string(v, "model")?);
    let seed = v
        .get("seed")
        .and_then(Value::as_u64)
        .ok_or("seed required")? as u32;
    let (host, engine) = if string(v, "side")? == "pr3" {
        (
            Some(
                Host::new(
                    Arc::new(LlamaBackend::load_seeded(model, seed).map_err(|e| e.to_string())?),
                    pr3_tool::documents::tools(library.clone()),
                )
                .map_err(|e| e.to_string())?
                .with_tasks(library.clone(), 30)
                .map_err(|e| e.to_string())?,
            ),
            None,
        )
    } else {
        (
            None,
            Some(
                Engine::load(
                    model,
                    EngineConfig {
                        seed,
                        gpu_layers: 999,
                        ..EngineConfig::default()
                    },
                )
                .map_err(|e| e.to_string())?,
            ),
        )
    };
    Ok(Run {
        host,
        engine,
        tools: pr3_tool::documents::tools(library),
        calls: 0,
        rounds: 0,
    })
}
fn request(run: &mut Run, v: &Value) -> Result<Value, String> {
    match string(v, "op")? {
        "specs" => Ok(json!(
            run.tools.iter().map(|t| t.spec()).collect::<Vec<_>>()
        )),
        "run" => {
            let events = Arc::new(Mutex::new(Vec::<TurnEvent>::new()));
            let captured = events.clone();
            let traces = Arc::new(Mutex::new(Vec::<pr3_core::Trace>::new()));
            let trace_capture = traces.clone();
            run.host
                .as_ref()
                .ok_or("PR3 not initialized")?
                .set_trace_sink(Arc::new(move |t| {
                    if let Ok(mut all) = trace_capture.lock() {
                        all.push(t.clone());
                    }
                }));
            let start = Instant::now();
            let outcome = block_on(run.host.as_ref().ok_or("PR3 not initialized")?.send(
                string(v, "question")?.into(),
                0,
                move |event| {
                    if let Ok(mut e) = captured.lock() {
                        e.push(event)
                    }
                },
            ));
            let (answer, error) = match outcome {
                Ok(actions) => (
                    actions
                        .into_iter()
                        .find_map(|a| match a {
                            pr3_core::Action::Reply { content, .. } => Some(content),
                            _ => None,
                        })
                        .unwrap_or_default(),
                    None,
                ),
                Err(e) => (String::new(), Some(e.to_string())),
            };
            Ok(
                json!({"answer":answer,"error":error,"seconds":start.elapsed().as_secs_f64(),"events":*events.lock().map_err(|_|"events lock")?,"traces":*traces.lock().map_err(|_|"traces lock")?}),
            )
        }
        "generate" => {
            run.rounds += 1;
            if run.rounds > 5 {
                return Err("Matched budget: at most 5 inference calls".into());
            }
            let mut messages = Vec::new();
            for m in v
                .get("messages")
                .and_then(Value::as_array)
                .ok_or("messages required")?
            {
                let role = match string(m, "role")? {
                    "system" => ChatRole::System,
                    "user" => ChatRole::User,
                    "assistant" => ChatRole::Assistant,
                    "tool" => ChatRole::Tool,
                    _ => return Err("unknown role".into()),
                };
                messages.push(ChatMessage::new(role, string(m, "content")?));
            }
            let result = run
                .engine
                .as_ref()
                .ok_or("OpenJarvis engine not initialized")?
                .chat_with_tools(&messages, &[], &mut |_| {})
                .map_err(|e| e.to_string())?;
            Ok(
                json!({"content":result.text,"usage":{"prompt_tokens":result.prompt_tokens,"completion_tokens":result.completion_tokens,"total_tokens":result.prompt_tokens+result.completion_tokens}}),
            )
        }
        "tool" => {
            run.calls += 1;
            if run.calls > 12 || run.rounds > 4 {
                return Err("Matched budget: 4 tool rounds / 12 calls".into());
            }
            let name = string(v, "name")?;
            let args = v
                .get("arguments")
                .and_then(Value::as_object)
                .ok_or("arguments object required")?;
            if serde_json::to_vec(args).map_err(|e| e.to_string())?.len() > 4096 {
                return Err("Matched argument size limit".into());
            }
            let tool = run
                .tools
                .iter()
                .find(|t| t.spec().name == name)
                .ok_or("Unknown tool; no action executed")?;
            let result = tool.run_with_sources(
                args,
                &ToolContext {
                    now: pr3_core::Timestamp(0),
                    utc_offset_minutes: 0,
                    session: pr3_core::SessionId(1),
                    utterance: String::new(),
                    earlier: Vec::new(),
                },
            );
            let success = result.result.is_ok();
            let output = result.result.unwrap_or_else(|e| e);
            Ok(
                json!({"name":name,"arguments":args,"success":success,"output":output,"sources":result.sources}),
            )
        }
        _ => Err("Unknown operation".into()),
    }
}
fn main() {
    let mut run = None;
    for line in io::stdin().lock().lines() {
        let result = (|| {
            let line = line.map_err(|e| e.to_string())?;
            let v: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
            if v.get("op").and_then(Value::as_str) == Some("init") {
                run = None;
                run = Some(init(&v)?);
                return Ok(json!({"ready":true}));
            }
            request(run.as_mut().ok_or("Initialize first")?, &v)
        })();
        let value = match result {
            Ok(v) => v,
            Err(e) => json!({"error":e}),
        };
        if writeln!(io::stdout(), "{value}")
            .and_then(|_| io::stdout().flush())
            .is_err()
        {
            break;
        }
    }
}
