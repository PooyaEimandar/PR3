//! Runs the app's complete correction-review path, including lexical candidates,
//! bounded semantic batches, validation and incomplete-review reporting.
use super::*;
use pr3_memory::{NewMemory, Provenance, Status};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    memories: Vec<String>,
    fact: String,
    matches: Vec<i64>,
}

#[derive(Serialize, Deserialize)]
struct Outcome {
    id: String,
    seed: u32,
    passed: bool,
    expected: Vec<i64>,
    selected: Vec<i64>,
    incomplete: bool,
    notice: Option<String>,
    seconds: f64,
    error: Option<String>,
}

pub fn run(args: &[String]) -> Result<(), String> {
    let options = Options::parse(args)?;
    let model = PathBuf::from(options.get("model").ok_or("--model is required")?);
    let corpus = PathBuf::from(options.get("corpus").ok_or("--corpus is required")?);
    let out = PathBuf::from(options.get("out").ok_or("--out is required")?);
    let seed = options.number("seed")?.unwrap_or(SEED);
    let cases: Vec<Case> = serde_json::from_slice(&fs::read(&corpus).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let ids: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
    if ids.is_empty() || ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err("empty or duplicate memory cases".into());
    }
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    manifest::bind(&out, &model, &corpus, seed, false, &ids)?;
    let mut done = BTreeSet::new();
    if out.exists() {
        for line in fs::read_to_string(&out).map_err(|e| e.to_string())?.lines() {
            let row: Outcome = serde_json::from_str(line).map_err(|e| e.to_string())?;
            if row.seed != seed || !ids.contains(&row.id.as_str()) || !done.insert(row.id) {
                return Err("foreign or duplicate memory result".into());
            }
        }
    }
    let backend: Arc<dyn Backend> =
        Arc::new(LlamaBackend::load_seeded(&model, seed).map_err(|e| e.to_string())?);
    for case in cases {
        if done.contains(&case.name) {
            continue;
        }
        let mut store = SqliteMemory::open_in_memory().map_err(|e| e.to_string())?;
        for text in case.memories {
            store
                .remember(NewMemory {
                    text,
                    status: Status::Confirmed,
                    provenance: Provenance::from_user(now()),
                })
                .map_err(|e| e.to_string())?;
        }
        let started = Instant::now();
        let result = block_on(pr3_app_lib::bench::review_memory(
            backend.clone(),
            Arc::new(Mutex::new(store)),
            case.fact,
        ));
        let (selected, incomplete, notice, error) = match result {
            Ok((ids, incomplete, notice)) => (ids, incomplete, notice, None),
            Err(e) => (vec![], true, None, Some(e)),
        };
        let row = Outcome {
            id: case.name,
            seed,
            passed: error.is_none() && !incomplete && selected == case.matches,
            expected: case.matches,
            selected,
            incomplete,
            notice,
            seconds: started.elapsed().as_secs_f64(),
            error,
        };
        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&out)
            .map_err(|e| e.to_string())?;
        writeln!(
            file,
            "{}",
            serde_json::to_string(&row).map_err(|e| e.to_string())?
        )
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
        eprintln!(
            "{}: passed={}, {:.3}s, selected={:?}, expected={:?}, incomplete={}",
            row.id, row.passed, row.seconds, row.selected, row.expected, row.incomplete
        );
    }
    Ok(())
}
