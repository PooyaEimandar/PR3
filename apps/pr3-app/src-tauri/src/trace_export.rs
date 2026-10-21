//! Writes PR3 traces as OpenJarvis trace records, one JSON object per line.
//!
//! Field for field this matches OpenJarvis's `_trace_to_dict`
//! (src/openjarvis/traces/analyzer.py at 6ae98f0): float seconds for times, the same
//! `step_type` strings and step keys, `outcome` as "success"/"failure". OpenJarvis's
//! trace analysis and learning can therefore read PR3 traces unchanged, which is what
//! matched benchmarks need (Roadmap §8.1). PR3-only details go under `metadata.pr3`.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use pr3_core::{Outcome, StepType, Timestamp, Trace};
use serde_json::{Value, json};

fn seconds(t: Timestamp) -> f64 {
    t.0 as f64 / 1000.0
}

pub fn to_openjarvis(trace: &Trace) -> Value {
    let steps: Vec<Value> = trace
        .steps
        .iter()
        .map(|s| {
            json!({
                "step_type": match s.step_type {
                    StepType::Generate => "generate",
                    StepType::ToolCall => "tool_call",
                    StepType::Respond => "respond",
                },
                "timestamp": seconds(s.at),
                "duration_seconds": s.duration_ms as f64 / 1000.0,
                "input": s.input,
                "output": s.output,
                "metadata": {},
            })
        })
        .collect();
    let total_latency_ms: u64 = trace.steps.iter().map(|s| s.duration_ms).sum();
    json!({
        "trace_id": trace.trace_id,
        "query": trace.query,
        "agent": trace.agent,
        "model": trace.model,
        "engine": trace.engine,
        "result": trace.result,
        "outcome": match trace.outcome {
            Outcome::Success => "success",
            Outcome::Failure => "failure",
        },
        "feedback": Value::Null,
        "started_at": seconds(trace.started_at),
        "ended_at": seconds(trace.ended_at),
        "total_tokens": trace.total_tokens,
        "total_latency_seconds": total_latency_ms as f64 / 1000.0,
        "metadata": { "pr3": { "version": env!("CARGO_PKG_VERSION") } },
        "steps": steps,
    })
}

/// Appends one trace to a JSON Lines file, creating it and its folder if needed.
pub fn append(path: &Path, trace: &Trace) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", to_openjarvis(trace))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pr3_core::TraceStep;

    fn trace() -> Trace {
        Trace {
            trace_id: "pr3-1-1".to_string(),
            query: "what is 2+2?".to_string(),
            agent: "pr3".to_string(),
            model: "Qwen3-0.6B-Q8_0".to_string(),
            engine: "llama.cpp".to_string(),
            steps: vec![
                TraceStep {
                    step_type: StepType::ToolCall,
                    at: Timestamp(2_500),
                    duration_ms: 1_500,
                    input: json!({ "tool": "calculate", "arguments": "{}" }),
                    output: json!({ "success": true, "result": "4" }),
                },
                TraceStep {
                    step_type: StepType::Respond,
                    at: Timestamp(3_000),
                    duration_ms: 0,
                    input: json!({}),
                    output: json!({ "content": "4", "turns": 2 }),
                },
            ],
            result: "4".to_string(),
            outcome: Outcome::Success,
            started_at: Timestamp(1_000),
            ended_at: Timestamp(3_000),
            total_tokens: 42,
        }
    }

    #[test]
    fn matches_openjarvis_field_names_and_units() {
        let v = to_openjarvis(&trace());
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "agent",
                "ended_at",
                "engine",
                "feedback",
                "metadata",
                "model",
                "outcome",
                "query",
                "result",
                "started_at",
                "steps",
                "total_latency_seconds",
                "total_tokens",
                "trace_id"
            ]
        );
        assert_eq!(v["outcome"], "success");
        assert_eq!(v["started_at"], 1.0);
        assert_eq!(v["total_latency_seconds"], 1.5);
        assert_eq!(v["steps"][0]["step_type"], "tool_call");
        assert_eq!(v["steps"][0]["timestamp"], 2.5);
        assert_eq!(v["steps"][0]["output"]["result"], "4");
    }

    #[test]
    fn append_writes_one_line_per_trace() {
        let dir = std::env::temp_dir().join(format!("pr3-trace-test-{}", std::process::id()));
        let path = dir.join("traces.jsonl");
        append(&path, &trace()).unwrap();
        append(&path, &trace()).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);
        let _ = fs::remove_dir_all(dir);
    }
}
