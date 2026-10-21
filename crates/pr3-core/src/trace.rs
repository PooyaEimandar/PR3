//! Turn traces. Shaped after OpenJarvis's `Trace`/`TraceStep` (src/openjarvis/core/
//! types.py) with the same step types and step keys, so an exporter can write records
//! OpenJarvis's learning and evaluation tools read unchanged. The core keeps integer
//! milliseconds; conversion to OpenJarvis's float seconds happens in the exporter.

use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::types::{GenerationInfo, ProposedToolCall, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    Generate,
    ToolCall,
    Respond,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Failure,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceStep {
    pub step_type: StepType,
    pub at: Timestamp,
    pub duration_ms: u64,
    pub input: Value,
    pub output: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trace {
    pub trace_id: String,
    pub query: String,
    pub agent: String,
    pub model: String,
    pub engine: String,
    pub steps: Vec<TraceStep>,
    pub result: String,
    pub outcome: Outcome,
    pub started_at: Timestamp,
    pub ended_at: Timestamp,
    pub total_tokens: u64,
}

#[derive(Debug)]
pub(crate) struct TraceBuilder {
    trace: Trace,
}

impl TraceBuilder {
    pub(crate) fn new(trace_id: String, query: String, at: Timestamp) -> Self {
        Self {
            trace: Trace {
                trace_id,
                query,
                agent: String::from("pr3"),
                model: String::new(),
                engine: String::new(),
                steps: Vec::new(),
                result: String::new(),
                outcome: Outcome::Failure,
                started_at: at,
                ended_at: at,
                total_tokens: 0,
            },
        }
    }

    fn push(
        &mut self,
        step_type: StepType,
        since: Timestamp,
        at: Timestamp,
        input: Value,
        output: Value,
    ) {
        self.trace.steps.push(TraceStep {
            step_type,
            at,
            duration_ms: at.0.saturating_sub(since.0),
            input,
            output,
        });
    }

    pub(crate) fn generate(
        &mut self,
        since: Timestamp,
        at: Timestamp,
        info: &GenerationInfo,
        content: &str,
        tool_calls: &[ProposedToolCall],
    ) {
        let total = u64::from(info.prompt_tokens) + u64::from(info.completion_tokens);
        self.trace.total_tokens += total;
        self.trace.model.clone_from(&info.model);
        self.trace.engine.clone_from(&info.engine);
        let calls: Vec<Value> = tool_calls
            .iter()
            .map(|c| json!({ "name": c.name, "arguments": c.arguments }))
            .collect();
        let finish_reason = if tool_calls.is_empty() {
            "stop"
        } else {
            "tool_calls"
        };
        self.push(
            StepType::Generate,
            since,
            at,
            json!({ "model": info.model }),
            json!({
                "content": content,
                "tool_calls": calls,
                "prompt_tokens": info.prompt_tokens,
                "completion_tokens": info.completion_tokens,
                "total_tokens": total,
                "tokens": total,
                "finish_reason": finish_reason,
            }),
        );
    }

    pub(crate) fn generate_failed(&mut self, since: Timestamp, at: Timestamp, reason: &str) {
        self.push(
            StepType::Generate,
            since,
            at,
            json!({ "model": self.trace.model }),
            json!({ "content": "", "error": reason, "finish_reason": "error", "tokens": 0 }),
        );
    }

    pub(crate) fn tool_call(
        &mut self,
        since: Timestamp,
        at: Timestamp,
        name: &str,
        arguments: &str,
        success: bool,
        result: &str,
    ) {
        self.push(
            StepType::ToolCall,
            since,
            at,
            json!({ "tool": name, "arguments": arguments }),
            json!({ "success": success, "result": result }),
        );
    }

    pub(crate) fn finish(
        mut self,
        outcome: Outcome,
        result: String,
        turns: u32,
        at: Timestamp,
    ) -> Trace {
        if outcome == Outcome::Success {
            self.push(
                StepType::Respond,
                at,
                at,
                json!({}),
                json!({ "content": result, "turns": turns }),
            );
        }
        self.trace.outcome = outcome;
        self.trace.result = result;
        self.trace.ended_at = at;
        self.trace
    }
}
