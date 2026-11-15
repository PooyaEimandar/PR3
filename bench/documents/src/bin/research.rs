//! Development research diagnostic. Real local inference, production Host/tools,
//! synthetic documents and explicitly marked offline web fixtures. No parity claim.
use pr3_app_lib::bench::{Host, LlamaBackend, TurnEvent, block_on};
use pr3_core::Action;
use pr3_memory_sqlite::SqliteMemory;
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Instant,
};
struct Pages {
    pages: Vec<(String, String)>,
    on: AtomicBool,
    reads: AtomicUsize,
}
impl pr3_tool::web::Fetch for Pages {
    fn check_enabled(&self) -> Result<(), String> {
        if self.on.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err("Fixture network offline after restart".into())
        }
    }
    fn fetch(&self, url: &str) -> Result<pr3_tool::web::Fetched, String> {
        self.check_enabled()?;
        self.reads.fetch_add(1, Ordering::SeqCst);
        let (_, text) = self
            .pages
            .iter()
            .find(|(u, _)| u == url)
            .ok_or("No fixture page for URL")?;
        Ok(pr3_tool::web::Fetched {
            url: url.into(),
            media_type: "text/plain".into(),
            body: text.as_bytes().to_vec(),
        })
    }
}
fn string<'a>(v: &'a Value, k: &str) -> Result<&'a str, String> {
    v[k].as_str().ok_or_else(|| format!("{k} required"))
}
fn quotations_match(answer: &str, expected: &Value) -> bool {
    let quoted = answer
        .lines()
        .filter_map(|line| line.strip_prefix("> "))
        .collect::<Vec<_>>()
        .join("\n");
    match expected {
        Value::Null => true, // Earlier corpora do not specify exact quotation checks.
        Value::Array(items) => items.iter().all(|v| {
            v.as_str()
                .is_some_and(|s| !s.is_empty() && quoted.contains(s))
        }),
        _ => false,
    }
}
fn report_evidence(answer: &str) -> (String, usize) {
    let mut sources = std::collections::BTreeSet::new();
    let mut text = Vec::new();
    for line in answer.lines() {
        if let Some(source) = line.strip_prefix("Source: ") {
            if let Some((_, id)) = source.rsplit_once(" [")
                && let Some(id) = id.strip_suffix(']')
            {
                sources.insert(id);
            }
        } else {
            text.push(line);
        }
    }
    (text.join("\n"), sources.len())
}
fn turn(host: &Host, request: &str, case: &Value) -> Value {
    let events = Arc::new(Mutex::new(Vec::<TurnEvent>::new()));
    let sink = events.clone();
    let traces = Arc::new(Mutex::new(Vec::<pr3_core::Trace>::new()));
    let trace_sink = traces.clone();
    host.set_trace_sink(Arc::new(move |trace| {
        if let Ok(mut t) = trace_sink.lock() {
            t.push(trace.clone());
        }
    }));
    let start = Instant::now();
    let outcome = block_on(host.send(request.into(), 0, move |e| {
        if let Ok(mut events) = sink.lock() {
            events.push(e);
        }
    }));
    let (answer, error) = match outcome {
        Ok(actions) => (
            actions
                .iter()
                .find_map(|a| match a {
                    Action::Reply { content, .. } => Some(content.clone()),
                    _ => None,
                })
                .unwrap_or_default(),
            actions.iter().find_map(|a| match a {
                Action::Failed { reason, .. } => Some(reason.clone()),
                _ => None,
            }),
        ),
        Err(e) => (String::new(), Some(e.to_string())),
    };
    let (report_text, citations) = report_evidence(&answer);
    let expected = case["expect"].as_array().is_none_or(|items| {
        items
            .iter()
            .all(|v| report_text.contains(v.as_str().unwrap_or_default()))
    });
    let absent = case["absent"].as_array().is_none_or(|items| {
        items
            .iter()
            .all(|v| !answer.contains(v.as_str().unwrap_or_default()))
    });
    let quotations = quotations_match(&answer, &case["quotes"]);
    let supported = answer.starts_with("Research findings")
        && citations >= case["citations"].as_u64().unwrap_or(0) as usize;
    json!({"answer":answer,"error":error,"seconds":start.elapsed().as_secs_f64(),"checks":{"expected":expected,"absent":absent,"report_and_citations":supported,"quotations":quotations},"pass":error.is_none()&&expected&&absent&&supported&&quotations,"events":events.lock().ok().as_deref(),"traces":traces.lock().ok().as_deref()})
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    let model = args
        .get(1)
        .ok_or("Usage: research MODEL SEED CASES OUTPUT")?;
    let seed = args
        .get(2)
        .ok_or("seed")?
        .parse::<u32>()
        .map_err(|e| e.to_string())?;
    let corpus: Value = serde_json::from_str(
        &fs::read_to_string(args.get(3).ok_or("cases")?).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(args.get(4).ok_or("output")?)
        .map_err(|e| e.to_string())?;
    let model =
        Arc::new(LlamaBackend::load_seeded(Path::new(model), seed).map_err(|e| e.to_string())?);
    for case in corpus["cases"].as_array().ok_or("cases array")? {
        let id = string(case, "id")?;
        let dir = std::env::temp_dir().join(format!(
            "pr3-research-diagnostic-{}-{id}",
            std::process::id()
        ));
        fs::create_dir(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("db.sqlite");
        let mut db = SqliteMemory::open(&path).map_err(|e| e.to_string())?;
        for d in case["documents"].as_array().ok_or("documents")? {
            db.import_document(string(d, "name")?, string(d, "text")?)
                .map_err(|e| e.to_string())?;
        }
        let db = Arc::new(Mutex::new(db));
        let pages = Arc::new(Pages {
            pages: case["pages"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|p| Ok((string(p, "url")?.into(), string(p, "text")?.into())))
                .collect::<Result<_, String>>()?,
            on: AtomicBool::new(true),
            reads: AtomicUsize::new(0),
        });
        let host = Host::new(
            model.clone(),
            pr3_tool::research::tools(db.clone(), Some(pages.clone()), None),
        )
        .map_err(|e| e.to_string())?
        .with_tasks(db.clone(), 30)
        .map_err(|e| e.to_string())?;
        let request = format!(
            "Research {}: {}",
            if case["online"] == true {
                "online"
            } else {
                "locally"
            },
            string(case, "topic")?
        );
        let mut turns = vec![turn(&host, &request, case)];
        let task = host
            .current_task()
            .map_err(|e| e.to_string())?
            .ok_or("task missing")?
            .card
            .id;
        drop(host);
        drop(db);
        if let Some(request) = case["restart"].as_str() {
            pages.on.store(false, Ordering::SeqCst);
            let db = Arc::new(Mutex::new(
                SqliteMemory::open(&path).map_err(|e| e.to_string())?,
            ));
            let host = Host::new(
                model.clone(),
                pr3_tool::research::tools(db.clone(), Some(pages.clone()), None),
            )
            .map_err(|e| e.to_string())?
            .with_tasks(db, 30)
            .map_err(|e| e.to_string())?;
            host.continue_task(pr3_core::SessionId(task))?;
            turns.push(turn(
                &host,
                request,
                case.get("restart_checks").unwrap_or(case),
            ));
        }
        let passed = turns.iter().all(|t| t["pass"] == true);
        writeln!(output,"{}",json!({"id":id,"seed":seed,"development":true,"web":"offline fixtures, no live provider","pass":passed,"page_fetches":pages.reads.load(Ordering::SeqCst),"turns":turns})).map_err(|e|e.to_string())?;
        output.flush().map_err(|e| e.to_string())?;
        eprintln!(
            "research {id} seed {seed}: {}",
            if passed { "PASS" } else { "FAIL" }
        );
        fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_macros)] // Tests may assert (clippy.toml).
mod tests {
    use super::*;
    #[test]
    fn metadata_and_duplicate_citations_cannot_supply_a_missing_fact_or_source() {
        let answer = "Research findings\n> The price is 29.\nSource: note36.md · line 1 [document:1:1:abc36]\n> The price is 29.\nSource: note36.md · line 1 [document:1:1:abc36]\nCoverage: Web search incomplete";
        let (text, citations) = report_evidence(answer);
        assert!(text.contains("29"));
        assert!(text.contains("Web search incomplete"));
        assert!(!text.contains("36"));
        assert_eq!(citations, 1);
        let (_, distinct) = report_evidence(&format!(
            "{answer}\nSource: other.md · line 1 [document:2:1:def]"
        ));
        assert_eq!(distinct, 2);
    }

    #[test]
    fn requested_quotations_must_be_in_evidence_not_just_analysis() {
        let report =
            "Analysis mentions requested instruction.\n\n> A relevant fact.\n> Another line.";
        assert!(!quotations_match(report, &json!(["requested instruction"])));
        assert!(quotations_match(
            report,
            &json!(["A relevant fact.\nAnother line."])
        ));
        assert!(!quotations_match(report, &json!(["A different fact."])));
        assert!(quotations_match(report, &Value::Null));
        assert!(!quotations_match(report, &json!("incorrect configuration")));
        assert!(!quotations_match(report, &json!([""])));
    }
}
