use std::fs;
use std::io::{Read as _, Write as _};
use std::path::Path;

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

fn digest(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn bind(
    out: &Path,
    model: &Path,
    corpus: &Path,
    seed: u32,
    web: bool,
    cases: &[&str],
) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let config = json!({
        "harness": 2, "model_sha256": digest(model)?,
        "binary_sha256": digest(&executable)?, "corpus_sha256": digest(corpus)?,
        "seed": seed, "web": web, "cases": cases,
    });
    ensure(out, &config)
}

fn ensure(out: &Path, config: &Value) -> Result<(), String> {
    let path = out.with_extension("manifest.json");
    if path.exists() {
        let old: Value = serde_json::from_slice(&fs::read(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if old != *config {
            return Err(format!(
                "Incompatible resume: {}; use a new output path",
                path.display()
            ));
        }
    } else {
        if out.metadata().is_ok_and(|m| m.len() > 0) {
            return Err(format!(
                "Existing results have no manifest: {}",
                out.display()
            ));
        }
        let bytes = serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resume_rejects_changed_seed_model_or_unidentified_output() {
        let dir = std::env::temp_dir().join(format!("pr3-task-manifest-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let out = dir.join("run.jsonl");
        let config = json!({"seed":1,"model":"a"});
        ensure(&out, &config).unwrap();
        ensure(&out, &config).unwrap();
        assert!(ensure(&out, &json!({"seed":2,"model":"a"})).is_err());
        assert!(ensure(&out, &json!({"seed":1,"model":"b"})).is_err());
        fs::remove_file(out.with_extension("manifest.json")).unwrap();
        fs::write(&out, "{}\n").unwrap();
        assert!(ensure(&out, &config).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
