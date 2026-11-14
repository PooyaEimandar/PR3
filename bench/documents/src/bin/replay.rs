//! Replay recorded model outputs through the final Host/tools, without inference.
//! Check that final runtime hardening preserves a frozen run's recorded answers.
use pr3_app_lib::bench::{Backend, Generation, Host, block_on};
use pr3_core::{Action, GenerationInfo, InferenceRequest, ProposedToolCall};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    sync::{Arc, Mutex},
};
struct Recorded(Mutex<VecDeque<Generation>>);
impl Backend for Recorded {
    fn label(&self) -> String {
        "recorded model outputs (no inference)".into()
    }
    fn generate(
        &self,
        _: &InferenceRequest,
        _: &mut dyn FnMut(&str),
    ) -> Result<Generation, String> {
        self.0
            .lock()
            .map_err(|_| "recording lock")?
            .pop_front()
            .ok_or("Unexpected inference request during replay".into())
    }
}
fn run() -> Result<Value, String> {
    let args: Vec<_> = std::env::args().collect();
    let corpus: Value = serde_json::from_str(
        &fs::read_to_string(args.get(1).ok_or("Usage: replay CORPUS RESULTS")?)
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let cases: BTreeMap<_, _> = corpus["cases"]
        .as_array()
        .ok_or("cases")?
        .iter()
        .map(|c| (c["id"].as_str().unwrap_or_default(), c))
        .collect();
    let mut replayed = Vec::new();
    for line in fs::read_to_string(args.get(2).ok_or("RESULTS required")?)
        .map_err(|e| e.to_string())?
        .lines()
    {
        let row: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if row["side"] != "pr3" {
            continue;
        }
        let id = row["id"].as_str().ok_or("id")?;
        let case = cases.get(id).ok_or("missing case")?;
        let mut db =
            pr3_memory_sqlite::SqliteMemory::open_in_memory().map_err(|e| e.to_string())?;
        for d in case["documents"].as_array().ok_or("documents")? {
            db.import_document(
                d["name"].as_str().ok_or("name")?,
                d["text"].as_str().ok_or("text")?,
            )
            .map_err(|e| e.to_string())?;
        }
        let trace = &row["traces"][0];
        let mut generations = VecDeque::new();
        for step in trace["steps"].as_array().ok_or("steps")? {
            if step["step_type"] != "generate" {
                continue;
            }
            let output = &step["output"];
            let tool_calls: Vec<ProposedToolCall> = serde_json::from_value(
                output["tool_calls"]
                    .as_array()
                    .cloned()
                    .map(Value::Array)
                    .unwrap_or_else(|| json!([])),
            )
            .map_err(|e| e.to_string())?;
            generations.push_back(Generation {
                content: output["content"].as_str().unwrap_or_default().into(),
                tool_calls,
                info: GenerationInfo::default(),
            });
        }
        let model = Arc::new(Recorded(Mutex::new(generations)));
        let library = Arc::new(Mutex::new(db));
        let host = Host::new(model.clone(), pr3_tool::documents::tools(library.clone()))
            .map_err(|e| e.to_string())?
            .with_tasks(library, 30)
            .map_err(|e| e.to_string())?;
        let actions =
            block_on(host.send(trace["query"].as_str().ok_or("query")?.into(), 0, |_| {}))
                .map_err(|e| e.to_string())?;
        let answer = actions
            .into_iter()
            .find_map(|a| match a {
                Action::Reply { content, .. } => Some(content),
                _ => None,
            })
            .ok_or("No replayed reply")?;
        if answer != row["answer"].as_str().ok_or("answer")?
            || !model.0.lock().map_err(|_| "recording lock")?.is_empty()
        {
            return Err(format!("Replay changed: {id} seed {}", row["seed"]));
        }
        replayed.push(json!({"id":id,"seed":row["seed"],"identical_answer":true}));
    }
    if replayed.len() != 36 {
        return Err(format!("Expected 36 PR3 runs, found {}", replayed.len()));
    }
    Ok(json!({"replayed":replayed.len(),"inference":false,"results":replayed}))
}
fn main() {
    match run() {
        Ok(out) => println!("{out}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
