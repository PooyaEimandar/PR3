//! Matched research only. Real Host / upstream agent, shared engine and read-only
//! research tools. Frozen network fixtures; no credentials or live network adapter.
use pr3_app_lib::bench::{Host, LlamaBackend, TurnEvent, block_on};
use pr3_core::{Action, Config, SessionId};
use pr3_llamacpp::{ChatMessage, ChatRole, Engine, EngineConfig, ToolCallText, ToolDef};
use pr3_memory_sqlite::{SqliteMemory, TaskRecord, TaskState};
use pr3_tool::{
    Tool, ToolContext,
    documents::Library,
    research::{SearchHit, SearchWeb},
    web::{Fetch, Fetched},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

fn field<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v[key].as_str().ok_or_else(|| format!("{key} required"))
}
struct Network {
    case: Value,
    offline: AtomicBool,
    log: Mutex<Vec<Value>>,
}
impl Fetch for Network {
    fn check_enabled(&self) -> Result<(), String> {
        if self.offline.load(Ordering::SeqCst) {
            Err("Frozen network is offline after restart".into())
        } else {
            Ok(())
        }
    }
    fn fetch(&self, url: &str) -> Result<Fetched, String> {
        self.log
            .lock()
            .map_err(|_| "network log")?
            .push(json!({"kind":"fetch","url":url,"offline":self.offline.load(Ordering::SeqCst)}));
        self.check_enabled()?;
        let page = self.case["pages"]
            .as_array()
            .and_then(|pages| pages.iter().find(|p| p["url"] == url))
            .ok_or("URL is outside frozen fixtures")?;
        Ok(Fetched {
            url: url.into(),
            media_type: "text/plain".into(),
            body: field(page, "text")?.as_bytes().to_vec(),
        })
    }
}
impl SearchWeb for Network {
    fn search(&self, topic: &str) -> Result<Vec<SearchHit>, String> {
        self.log.lock().map_err(|_| "network log")?.push(
            json!({"kind":"search","topic":topic,"offline":self.offline.load(Ordering::SeqCst)}),
        );
        self.check_enabled()?;
        if topic != field(&self.case, "topic")? {
            return Err("Only the frozen user topic may go to search".into());
        }
        if let Some(error) = self.case["search_error"].as_str() {
            return Err(error.into());
        }
        self.case["pages"]
            .as_array()
            .into_iter()
            .flatten()
            .take(5)
            .map(|p| {
                Ok(SearchHit {
                    url: field(p, "url")?.into(),
                    title: field(p, "title")?.into(),
                    snippet: p["snippet"]
                        .as_str()
                        .unwrap_or("Read this source for evidence.")
                        .into(),
                })
            })
            .collect()
    }
}
#[derive(Default)]
struct Budget {
    rounds: usize,
    calls: usize,
    round_calls: usize,
}
impl Budget {
    fn generate(&mut self) -> Result<(), String> {
        if self.rounds > Config::default().max_tool_rounds as usize {
            return Err("Matched inference round limit".into());
        }
        self.rounds += 1;
        self.round_calls = 0;
        Ok(())
    }
    fn tool(&mut self, bytes: usize) -> Result<(), String> {
        let limits = Config::default();
        self.calls += 1;
        self.round_calls += 1;
        if self.rounds == 0
            || self.rounds > limits.max_tool_rounds as usize
            || self.calls > limits.max_tool_calls_per_turn
            || self.round_calls > limits.max_tool_calls_per_round
            || bytes > limits.max_tool_args_bytes
        {
            return Err("Matched tool round/call/argument limit; nothing executed".into());
        }
        Ok(())
    }
}
struct Run {
    host: Option<Host>,
    backend: Option<Arc<LlamaBackend>>,
    engine: Option<Engine>,
    library: Library,
    tools: Vec<Box<dyn Tool>>,
    network: Arc<Network>,
    budget: Budget,
    path: PathBuf,
    question: String,
    task: u64,
}
fn toolset(library: &Library, network: &Arc<Network>) -> Vec<Box<dyn Tool>> {
    pr3_tool::research::tools(
        library.clone(),
        Some(network.clone()),
        Some(network.clone()),
    )
}
impl Run {
    fn new(v: &Value, path: PathBuf) -> Result<Self, String> {
        let case = &v["case"];
        let mut db = SqliteMemory::open(&path).map_err(|e| e.to_string())?;
        for d in case["documents"].as_array().ok_or("documents required")? {
            db.import_document(field(d, "name")?, field(d, "text")?)
                .map_err(|e| e.to_string())?;
        }
        let library = Arc::new(Mutex::new(db));
        let network = Arc::new(Network {
            case: case.clone(),
            offline: AtomicBool::new(false),
            log: Mutex::new(vec![]),
        });
        let model = Path::new(field(v, "model")?);
        let seed =
            u32::try_from(v["seed"].as_u64().ok_or("seed required")?).map_err(|e| e.to_string())?;
        let (host, backend, engine) = match field(v, "side")? {
            "pr3" => {
                let backend =
                    Arc::new(LlamaBackend::load_seeded(model, seed).map_err(|e| e.to_string())?);
                let host = Host::new(backend.clone(), toolset(&library, &network))
                    .map_err(|e| e.to_string())?
                    .with_tasks(library.clone(), 30)
                    .map_err(|e| e.to_string())?;
                (Some(host), Some(backend), None)
            }
            "openjarvis" => {
                let mut db = library.lock().map_err(|_| "storage")?;
                db.save_task(&TaskRecord {
                    id: 1,
                    title: field(case, "topic")?.into(),
                    parent: None,
                    state: TaskState::Working,
                    updated_at: pr3_memory::Timestamp(0),
                })
                .map_err(|e| e.to_string())?;
                db.begin_research(1, field(case, "topic")?, case["online"] == true)
                    .map_err(|e| e.to_string())?;
                (
                    None,
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
            }
            _ => return Err("Unknown comparison side".into()),
        };
        Ok(Self {
            tools: toolset(&library, &network),
            host,
            backend,
            engine,
            library,
            network,
            budget: Budget::default(),
            path,
            question: String::new(),
            task: 1,
        })
    }
    fn restart(&mut self) -> Result<(), String> {
        self.host = None;
        self.tools.clear();
        // Replace the last database handle before reopening it: real storage restart.
        self.library = Arc::new(Mutex::new(
            SqliteMemory::open_in_memory().map_err(|e| e.to_string())?,
        ));
        self.library = Arc::new(Mutex::new(
            SqliteMemory::open(&self.path).map_err(|e| e.to_string())?,
        ));
        self.network.offline.store(true, Ordering::SeqCst);
        self.tools = toolset(&self.library, &self.network);
        if let Some(backend) = &self.backend {
            let host = Host::new(backend.clone(), toolset(&self.library, &self.network))
                .map_err(|e| e.to_string())?
                .with_tasks(self.library.clone(), 30)
                .map_err(|e| e.to_string())?;
            host.continue_task(SessionId(self.task))?;
            self.host = Some(host);
        }
        self.budget = Budget::default();
        Ok(())
    }
    fn request(&mut self, v: &Value) -> Result<Value, String> {
        match field(v, "op")? {
            "specs" => Ok(json!(
                self.tools.iter().map(|t| t.spec()).collect::<Vec<_>>()
            )),
            "begin" => {
                self.question = field(v, "question")?.into();
                self.budget = Budget::default();
                Ok(json!({"ready":true}))
            }
            "restart" => {
                self.restart()?;
                Ok(json!({"ready":true}))
            }
            "evidence" => {
                let db = self.library.lock().map_err(|_| "storage")?;
                Ok(
                    json!({"sources":db.research_sources(self.task).map_err(|e|e.to_string())?,"network":*self.network.log.lock().map_err(|_|"network")?}),
                )
            }
            "run" => {
                let host = self.host.as_ref().ok_or("PR3 not initialized")?;
                let events = Arc::new(Mutex::new(Vec::<TurnEvent>::new()));
                let capture = events.clone();
                let traces = Arc::new(Mutex::new(Vec::<pr3_core::Trace>::new()));
                let trace_capture = traces.clone();
                host.set_trace_sink(Arc::new(move |t| {
                    if let Ok(mut all) = trace_capture.lock() {
                        all.push(t.clone());
                    }
                }));
                let start = Instant::now();
                let first = Arc::new(Mutex::new(None));
                let first_capture = first.clone();
                let outcome = block_on(host.send(field(v, "question")?.into(), 0, move |event| {
                    if matches!(event, TurnEvent::Token { .. } | TurnEvent::Result { .. })
                        && let Ok(mut time) = first_capture.lock()
                    {
                        time.get_or_insert(start.elapsed().as_secs_f64());
                    }
                    if let Ok(mut all) = capture.lock() {
                        all.push(event);
                    }
                }));
                let (answer, error) = match outcome {
                    Ok(actions) => (
                        actions
                            .iter()
                            .find_map(|a| {
                                if let Action::Reply { content, .. } = a {
                                    Some(content.clone())
                                } else {
                                    None
                                }
                            })
                            .unwrap_or_default(),
                        actions.iter().find_map(|a| {
                            if let Action::Failed { reason, .. } = a {
                                Some(reason.clone())
                            } else {
                                None
                            }
                        }),
                    ),
                    Err(e) => (String::new(), Some(e.to_string())),
                };
                if let Some(task) = host.current_task()? {
                    self.task = task.card.id;
                }
                Ok(
                    json!({"answer":answer,"error":error,"seconds":start.elapsed().as_secs_f64(),"first_visible_seconds":*first.lock().map_err(|_|"time")?,"events":*events.lock().map_err(|_|"events")?,"traces":*traces.lock().map_err(|_|"traces")?}),
                )
            }
            "generate" => {
                self.budget.generate()?;
                let mut messages = Vec::new();
                for m in v["messages"].as_array().ok_or("messages required")? {
                    let role = match field(m, "role")? {
                        "system" => ChatRole::System,
                        "user" => ChatRole::User,
                        "assistant" => ChatRole::Assistant,
                        "tool" => ChatRole::Tool,
                        _ => return Err("unknown role".into()),
                    };
                    let mut message = ChatMessage::new(role, m["content"].as_str().unwrap_or(""));
                    for tc in m["tool_calls"].as_array().into_iter().flatten() {
                        message.tool_calls.push(ToolCallText {
                            name: field(tc, "name")?.into(),
                            arguments: field(tc, "arguments")?.into(),
                        });
                    }
                    messages.push(message);
                }
                let specs: Vec<_> = self
                    .tools
                    .iter()
                    .map(|tool| {
                        let s = tool.spec();
                        ToolDef {
                            read_only: true,
                            report: s.name == pr3_tool::research::ANSWER,
                            name: s.name,
                            description: s.description,
                            parameters: s.parameters,
                        }
                    })
                    .collect();
                let start = Instant::now();
                let output = self
                    .engine
                    .as_ref()
                    .ok_or("OpenJarvis engine unavailable")?
                    .chat_with_tools(&messages, &specs, &mut |_| {})
                    .map_err(|e| e.to_string())?;
                Ok(
                    json!({"content":output.text,"tool_calls":output.tool_calls.iter().enumerate().map(|(i,tc)|json!({"id":format!("call_{}_{}",self.budget.rounds,i),"name":tc.name,"arguments":tc.arguments})).collect::<Vec<_>>(),"usage":{"prompt_tokens":output.prompt_tokens,"completion_tokens":output.completion_tokens,"total_tokens":output.prompt_tokens+output.completion_tokens},"inference_passes":output.inference_passes,"seconds":start.elapsed().as_secs_f64()}),
                )
            }
            "tool" => {
                let args = v["arguments"]
                    .as_object()
                    .ok_or("arguments object required")?;
                self.budget
                    .tool(serde_json::to_vec(args).map_err(|e| e.to_string())?.len())?;
                let name = field(v, "name")?;
                let tool = self
                    .tools
                    .iter()
                    .find(|t| t.spec().name == name)
                    .ok_or("Unknown tool; nothing executed")?;
                if tool.spec().requires_confirmation {
                    return Err("No action authority in research comparison".into());
                }
                let start = Instant::now();
                let output = tool.run_with_sources(
                    args,
                    &ToolContext {
                        now: pr3_core::Timestamp(0),
                        utc_offset_minutes: 0,
                        session: SessionId(self.task),
                        utterance: self.question.clone(),
                        earlier: vec![],
                    },
                );
                let success = output.result.is_ok();
                let text = output.result.unwrap_or_else(|e| e);
                if text.len() > Config::default().max_tool_output_bytes {
                    return Err("Matched output size limit".into());
                }
                Ok(
                    json!({"type":"tool","name":name,"arguments":args,"success":success,"output":text,"sources":output.sources,"seconds":start.elapsed().as_secs_f64()}),
                )
            }
            _ => Err("Unknown operation".into()),
        }
    }
}
fn main() {
    let dir = std::env::temp_dir().join(format!("pr3-matched-research-{}", std::process::id()));
    if let Err(e) = fs::create_dir(&dir) {
        eprintln!("{e}");
        std::process::exit(1);
    }
    let mut run: Option<Run> = None;
    let mut sequence = 0;
    for line in io::stdin().lock().lines() {
        let result = (|| {
            let value: Value = serde_json::from_str(&line.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            if value["op"] == "init" {
                run = None;
                sequence += 1;
                run = Some(Run::new(&value, dir.join(format!("{sequence}.sqlite")))?);
                return Ok(
                    json!({"ready":true,"limits":{"tool_rounds":Config::default().max_tool_rounds,"tool_calls":Config::default().max_tool_calls_per_turn,"calls_per_round":Config::default().max_tool_calls_per_round}}),
                );
            }
            run.as_mut().ok_or("Initialize first")?.request(&value)
        })();
        let value = result.unwrap_or_else(|e: String| json!({"error":e}));
        if writeln!(io::stdout(), "{value}")
            .and_then(|()| io::stdout().flush())
            .is_err()
        {
            break;
        }
    }
    drop(run);
    let _ = fs::remove_dir_all(dir);
}

#[cfg(test)]
#[allow(clippy::disallowed_macros)]
mod tests {
    use super::*;
    #[test]
    fn the_comparison_cannot_extend_runtime_budgets() {
        let mut b = Budget::default();
        assert!(b.tool(1).is_err());
        b = Budget::default();
        for _ in 0..3 {
            b.generate().unwrap();
            for _ in 0..4 {
                b.tool(4096).unwrap();
            }
        }
        b.generate().unwrap();
        assert!(b.tool(1).is_err());
        b.generate().unwrap();
        assert!(b.tool(1).is_err());
        assert!(b.generate().is_err());
        let mut b = Budget::default();
        b.generate().unwrap();
        for _ in 0..4 {
            b.tool(1).unwrap();
        }
        assert!(b.tool(1).is_err());
        let mut b = Budget::default();
        b.generate().unwrap();
        assert!(b.tool(4097).is_err());
    }
    #[test]
    fn fixtures_refuse_changed_search_topics_unlisted_pages_and_offline_reads() {
        let network = Network {
            case: json!({"topic":"public topic","pages":[]}),
            offline: AtomicBool::new(false),
            log: Mutex::new(vec![]),
        };
        assert!(network.search("private model query").is_err());
        assert!(network.fetch("https://attacker.example/").is_err());
        assert!(network.search("public topic").unwrap().is_empty());
        network.offline.store(true, Ordering::SeqCst);
        assert!(network.search("public topic").is_err());
    }
}
