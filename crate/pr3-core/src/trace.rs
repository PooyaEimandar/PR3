//! Turn traces. Shaped after OpenJarvis's `Trace`/`TraceStep` (src/openjarvis/core/
//! types.py) with the same step types and step keys, so an exporter can write records
//! OpenJarvis's learning and evaluation tools read unchanged. The core keeps integer
//! milliseconds; conversion to OpenJarvis's float seconds happens in the exporter.

use alloc::string::String;
use alloc::vec::Vec;

use serde::ser::{SerializeMap, SerializeSeq, SerializeStruct};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

use crate::history::History;
use crate::text::{push_clipped, push_json_str, push_u64};
use crate::types::{GenerationInfo, ProposedToolCall, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    /// Context looked up for the turn (OpenJarvis's memory retrieval step).
    Retrieve,
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

/// A stretch of [`TraceBuilder::text`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Span {
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Retrieve {
        query: Span,
        results: usize,
    },
    Generate {
        model: Span,
        content: Span,
        /// The proposals, as `TraceBuilder::calls[first..end]`.
        calls: (usize, usize),
        prompt_tokens: u32,
        completion_tokens: u32,
    },
    GenerateFailed {
        model: Span,
        reason: Span,
    },
    /// A call and its result, as the session's history holds them: the trace refers to
    /// them there instead of keeping copies. The exchange in progress is never dropped
    /// from the history, so they stay while the turn runs and while its trace is lent.
    ToolCall {
        /// Call `.1` of the message numbered `.0`; `None` for the note on calls over the
        /// per-message limit, which has no call.
        call: Option<(u64, usize)>,
        success: bool,
        /// The message with the result.
        result: u64,
    },
    Respond {
        content: Span,
        turns: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Step {
    at: Timestamp,
    duration_ms: u64,
    kind: Kind,
}

impl Step {
    fn step_type(&self) -> StepType {
        match self.kind {
            Kind::Retrieve { .. } => StepType::Retrieve,
            Kind::Generate { .. } | Kind::GenerateFailed { .. } => StepType::Generate,
            Kind::ToolCall { .. } => StepType::ToolCall,
            Kind::Respond { .. } => StepType::Respond,
        }
    }
}

/// A turn's trace as the core records it: typed steps over one text buffer, kept from
/// turn to turn so a steady-state turn allocates nothing. It writes exactly the JSON a
/// [`Trace`] writes, and [`TraceView::to_owned`] builds the [`Trace`] itself on request.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TraceBuilder {
    text: String,
    trace_id: Span,
    query: Span,
    model: Span,
    engine: Span,
    result: Span,
    steps: Vec<Step>,
    /// Every generation's proposals, as (name, arguments).
    calls: Vec<(Span, Span)>,
    outcome: Outcome,
    started_at: Timestamp,
    ended_at: Timestamp,
    total_tokens: u64,
}

impl Default for TraceBuilder {
    fn default() -> Self {
        Self {
            text: String::new(),
            trace_id: Span::default(),
            query: Span::default(),
            model: Span::default(),
            engine: Span::default(),
            result: Span::default(),
            steps: Vec::new(),
            calls: Vec::new(),
            outcome: Outcome::Failure,
            started_at: Timestamp(0),
            ended_at: Timestamp(0),
            total_tokens: 0,
        }
    }
}

impl TraceBuilder {
    /// Starts the trace of a new turn, keeping the buffers.
    pub(crate) fn begin(&mut self, session: u64, turn: u64, query: &str, at: Timestamp) {
        self.text.clear();
        self.steps.clear();
        self.calls.clear();
        self.text.push_str("pr3-");
        push_u64(&mut self.text, session);
        self.text.push('-');
        push_u64(&mut self.text, turn);
        self.trace_id = Span {
            start: 0,
            end: self.text.len(),
        };
        self.query = self.push_text(query);
        self.model = Span::default();
        self.engine = Span::default();
        self.result = Span::default();
        self.outcome = Outcome::Failure;
        self.started_at = at;
        self.ended_at = at;
        self.total_tokens = 0;
    }

    fn push_text(&mut self, text: &str) -> Span {
        let start = self.text.len();
        self.text.push_str(text);
        Span {
            start,
            end: self.text.len(),
        }
    }

    fn push_clipped(&mut self, text: &str, max: usize) -> Span {
        let start = self.text.len();
        push_clipped(&mut self.text, text, max);
        Span {
            start,
            end: self.text.len(),
        }
    }

    fn str(&self, span: Span) -> &str {
        self.text.get(span.start..span.end).unwrap_or("")
    }

    fn push(&mut self, since: Timestamp, at: Timestamp, kind: Kind) {
        self.steps.push(Step {
            at,
            duration_ms: at.0.saturating_sub(since.0),
            kind,
        });
    }

    /// A model step. The content and the proposals are recorded within the same bounds
    /// as the history.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn generate(
        &mut self,
        since: Timestamp,
        at: Timestamp,
        info: &GenerationInfo<'_>,
        content: &str,
        content_max: usize,
        tool_calls: &[ProposedToolCall<'_>],
        name_max: usize,
        args_max: usize,
    ) {
        let total = u64::from(info.prompt_tokens) + u64::from(info.completion_tokens);
        self.total_tokens += total;
        self.model = self.push_text(info.model);
        self.engine = self.push_text(info.engine);
        let content = self.push_clipped(content, content_max);
        let first = self.calls.len();
        for call in tool_calls {
            let name = self.push_clipped(call.name, name_max);
            let arguments = self.push_clipped(call.arguments, args_max);
            self.calls.push((name, arguments));
        }
        let kind = Kind::Generate {
            model: self.model,
            content,
            calls: (first, self.calls.len()),
            prompt_tokens: info.prompt_tokens,
            completion_tokens: info.completion_tokens,
        };
        self.push(since, at, kind);
    }

    pub(crate) fn generate_failed(&mut self, since: Timestamp, at: Timestamp, reason: &str) {
        let reason = self.push_text(reason);
        let kind = Kind::GenerateFailed {
            model: self.model,
            reason,
        };
        self.push(since, at, kind);
    }

    /// The host's lookup for this turn: what was searched and how much was found.
    pub(crate) fn retrieve(&mut self, at: Timestamp, query: &str, results: usize) {
        let query = self.push_text(query);
        self.push(at, at, Kind::Retrieve { query, results });
    }

    /// A tool step, by where the history holds the call and its result (see
    /// [`Kind::ToolCall`]).
    pub(crate) fn tool_call(
        &mut self,
        since: Timestamp,
        at: Timestamp,
        call: Option<(u64, usize)>,
        success: bool,
        result: u64,
    ) {
        let kind = Kind::ToolCall {
            call,
            success,
            result,
        };
        self.push(since, at, kind);
    }

    /// Ends the trace; `result` is recorded cut to `result_max` bytes.
    pub(crate) fn finish(
        &mut self,
        outcome: Outcome,
        result: &str,
        result_max: usize,
        turns: u32,
        at: Timestamp,
    ) {
        let result = self.push_clipped(result, result_max);
        if outcome == Outcome::Success {
            self.push(
                at,
                at,
                Kind::Respond {
                    content: result,
                    turns,
                },
            );
        }
        self.outcome = outcome;
        self.result = result;
        self.ended_at = at;
    }
}

/// The tool named by the step for calls over the per-message limit, which ran nothing.
pub(crate) const OVER_LIMIT: &str = "(calls over the limit)";

/// A finished turn's trace, lent by the core with the session's history, which holds its
/// tool calls and results. It serializes to exactly the JSON of the [`Trace`] that
/// [`Self::to_owned`] builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceView<'c> {
    pub(crate) trace: &'c TraceBuilder,
    pub(crate) history: &'c History,
}

impl<'c> TraceView<'c> {
    pub fn trace_id(&self) -> &'c str {
        self.s(self.trace.trace_id)
    }

    pub fn result(&self) -> &'c str {
        self.s(self.trace.result)
    }

    pub fn outcome(&self) -> Outcome {
        self.trace.outcome
    }

    pub fn step_count(&self) -> usize {
        self.trace.steps.len()
    }

    fn s(&self, span: Span) -> &'c str {
        self.trace.str(span)
    }

    /// A tool step's tool and arguments.
    fn tool(&self, call: Option<(u64, usize)>) -> (&'c str, &'c str) {
        match call {
            None => (OVER_LIMIT, ""),
            Some((seq, index)) => self
                .history
                .call(seq, index)
                .map_or(("", ""), |c| (c.name.as_str(), c.arguments.as_str())),
        }
    }

    /// A tool step's result.
    fn tool_result(&self, seq: u64) -> &'c str {
        self.history.message(seq).map_or("", |m| m.content.as_str())
    }

    /// The trace as JSON, byte for byte what `serde_json` writes for it, into `out`.
    /// Appends the trace as JSON, exactly what `serde_json` writes for it, without
    /// allocating once `out` has grown to fit.
    pub fn write_json(&self, out: &mut String) {
        let field = |out: &mut String, name: &str, text: &str| {
            out.push_str(name);
            push_json_str(out, text);
        };
        field(out, "{\"trace_id\":", self.s(self.trace.trace_id));
        field(out, ",\"query\":", self.s(self.trace.query));
        out.push_str(",\"agent\":\"pr3\"");
        field(out, ",\"model\":", self.s(self.trace.model));
        field(out, ",\"engine\":", self.s(self.trace.engine));
        out.push_str(",\"steps\":[");
        for (i, step) in self.trace.steps.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"step_type\":\"");
            out.push_str(match step.step_type() {
                StepType::Retrieve => "retrieve",
                StepType::Generate => "generate",
                StepType::ToolCall => "tool_call",
                StepType::Respond => "respond",
            });
            out.push_str("\",\"at\":");
            push_u64(out, step.at.0);
            out.push_str(",\"duration_ms\":");
            push_u64(out, step.duration_ms);
            out.push_str(",\"input\":");
            self.write_io(out, step, false);
            out.push_str(",\"output\":");
            self.write_io(out, step, true);
            out.push('}');
        }
        field(out, "],\"result\":", self.s(self.trace.result));
        out.push_str(match self.trace.outcome {
            Outcome::Success => ",\"outcome\":\"success\"",
            Outcome::Failure => ",\"outcome\":\"failure\"",
        });
        out.push_str(",\"started_at\":");
        push_u64(out, self.trace.started_at.0);
        out.push_str(",\"ended_at\":");
        push_u64(out, self.trace.ended_at.0);
        out.push_str(",\"total_tokens\":");
        push_u64(out, self.trace.total_tokens);
        out.push('}');
    }

    /// A step's input or output, as [`Io`] serializes it.
    fn write_io(&self, out: &mut String, step: &Step, output: bool) {
        let s = |span| self.s(span);
        match (step.kind, output) {
            (Kind::Retrieve { query, .. }, false) => {
                out.push_str("{\"query\":");
                push_json_str(out, s(query));
            }
            (Kind::Retrieve { results, .. }, true) => {
                out.push_str("{\"num_results\":");
                push_u64(out, results as u64);
            }
            (Kind::Generate { model, .. } | Kind::GenerateFailed { model, .. }, false) => {
                out.push_str("{\"model\":");
                push_json_str(out, s(model));
            }
            (
                Kind::Generate {
                    content,
                    calls,
                    prompt_tokens,
                    completion_tokens,
                    ..
                },
                true,
            ) => {
                let total = u64::from(prompt_tokens) + u64::from(completion_tokens);
                out.push_str("{\"completion_tokens\":");
                push_u64(out, u64::from(completion_tokens));
                out.push_str(",\"content\":");
                push_json_str(out, s(content));
                out.push_str(if calls.0 == calls.1 {
                    ",\"finish_reason\":\"stop\""
                } else {
                    ",\"finish_reason\":\"tool_calls\""
                });
                out.push_str(",\"prompt_tokens\":");
                push_u64(out, u64::from(prompt_tokens));
                out.push_str(",\"tokens\":");
                push_u64(out, total);
                out.push_str(",\"tool_calls\":[");
                let proposed = self.trace.calls.get(calls.0..calls.1).unwrap_or(&[]);
                for (i, &(name, arguments)) in proposed.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str("{\"arguments\":");
                    push_json_str(out, s(arguments));
                    out.push_str(",\"name\":");
                    push_json_str(out, s(name));
                    out.push('}');
                }
                out.push_str("],\"total_tokens\":");
                push_u64(out, total);
            }
            (Kind::GenerateFailed { reason, .. }, true) => {
                out.push_str("{\"content\":\"\",\"error\":");
                push_json_str(out, s(reason));
                out.push_str(",\"finish_reason\":\"error\",\"tokens\":0");
            }
            (Kind::ToolCall { call, .. }, false) => {
                let (tool, arguments) = self.tool(call);
                out.push_str("{\"arguments\":");
                push_json_str(out, arguments);
                out.push_str(",\"tool\":");
                push_json_str(out, tool);
            }
            (
                Kind::ToolCall {
                    success, result, ..
                },
                true,
            ) => {
                out.push_str("{\"result\":");
                push_json_str(out, self.tool_result(result));
                out.push_str(if success {
                    ",\"success\":true"
                } else {
                    ",\"success\":false"
                });
            }
            (Kind::Respond { .. }, false) => out.push('{'),
            (Kind::Respond { content, turns }, true) => {
                out.push_str("{\"content\":");
                push_json_str(out, s(content));
                out.push_str(",\"turns\":");
                push_u64(out, u64::from(turns));
            }
        }
        out.push('}');
    }

    fn io(&self, step: &Step, output: bool) -> Value {
        serde_json::to_value(Io {
            view: *self,
            step,
            output,
        })
        .unwrap_or(Value::Null)
    }

    pub fn to_owned(&self) -> Trace {
        Trace {
            trace_id: self.s(self.trace.trace_id).into(),
            query: self.s(self.trace.query).into(),
            agent: String::from("pr3"),
            model: self.s(self.trace.model).into(),
            engine: self.s(self.trace.engine).into(),
            steps: self
                .trace
                .steps
                .iter()
                .map(|step| TraceStep {
                    step_type: step.step_type(),
                    at: step.at,
                    duration_ms: step.duration_ms,
                    input: self.io(step, false),
                    output: self.io(step, true),
                })
                .collect(),
            result: self.s(self.trace.result).into(),
            outcome: self.trace.outcome,
            started_at: self.trace.started_at,
            ended_at: self.trace.ended_at,
            total_tokens: self.trace.total_tokens,
        }
    }
}

impl Serialize for TraceView<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let (v, t) = (*self, self.trace);
        let mut out = s.serialize_struct("Trace", 11)?;
        out.serialize_field("trace_id", v.s(t.trace_id))?;
        out.serialize_field("query", v.s(t.query))?;
        out.serialize_field("agent", "pr3")?;
        out.serialize_field("model", v.s(t.model))?;
        out.serialize_field("engine", v.s(t.engine))?;
        out.serialize_field("steps", &Steps(v))?;
        out.serialize_field("result", v.s(t.result))?;
        out.serialize_field("outcome", &t.outcome)?;
        out.serialize_field("started_at", &t.started_at)?;
        out.serialize_field("ended_at", &t.ended_at)?;
        out.serialize_field("total_tokens", &t.total_tokens)?;
        out.end()
    }
}

struct Steps<'c>(TraceView<'c>);

impl Serialize for Steps<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_seq(Some(self.0.trace.steps.len()))?;
        for step in &self.0.trace.steps {
            out.serialize_element(&StepOut(self.0, step))?;
        }
        out.end()
    }
}

struct StepOut<'c>(TraceView<'c>, &'c Step);

impl Serialize for StepOut<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let (view, step) = (self.0, self.1);
        let mut out = s.serialize_struct("TraceStep", 5)?;
        out.serialize_field("step_type", &step.step_type())?;
        out.serialize_field("at", &step.at)?;
        out.serialize_field("duration_ms", &step.duration_ms)?;
        let io = |output| Io { view, step, output };
        out.serialize_field("input", &io(false))?;
        out.serialize_field("output", &io(true))?;
        out.end()
    }
}

/// A step's input or output object, keys in the sorted order `serde_json` writes a map in.
struct Io<'c> {
    view: TraceView<'c>,
    step: &'c Step,
    output: bool,
}

impl Serialize for Io<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let t = self.view;
        match (self.step.kind, self.output) {
            (Kind::Retrieve { query, .. }, false) => {
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("query", t.s(query))?;
                m.end()
            }
            (Kind::Retrieve { results, .. }, true) => {
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("num_results", &results)?;
                m.end()
            }
            (Kind::Generate { model, .. } | Kind::GenerateFailed { model, .. }, false) => {
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("model", t.s(model))?;
                m.end()
            }
            (
                Kind::Generate {
                    content,
                    calls,
                    prompt_tokens,
                    completion_tokens,
                    ..
                },
                true,
            ) => {
                let total = u64::from(prompt_tokens) + u64::from(completion_tokens);
                let finish_reason = if calls.0 == calls.1 {
                    "stop"
                } else {
                    "tool_calls"
                };
                let mut m = s.serialize_map(Some(7))?;
                m.serialize_entry("completion_tokens", &completion_tokens)?;
                m.serialize_entry("content", t.s(content))?;
                m.serialize_entry("finish_reason", finish_reason)?;
                m.serialize_entry("prompt_tokens", &prompt_tokens)?;
                m.serialize_entry("tokens", &total)?;
                m.serialize_entry("tool_calls", &Calls(t, calls))?;
                m.serialize_entry("total_tokens", &total)?;
                m.end()
            }
            (Kind::GenerateFailed { reason, .. }, true) => {
                let mut m = s.serialize_map(Some(4))?;
                m.serialize_entry("content", "")?;
                m.serialize_entry("error", t.s(reason))?;
                m.serialize_entry("finish_reason", "error")?;
                m.serialize_entry("tokens", &0u64)?;
                m.end()
            }
            (Kind::ToolCall { call, .. }, false) => {
                let (tool, arguments) = t.tool(call);
                let mut m = s.serialize_map(Some(2))?;
                m.serialize_entry("arguments", arguments)?;
                m.serialize_entry("tool", tool)?;
                m.end()
            }
            (
                Kind::ToolCall {
                    success, result, ..
                },
                true,
            ) => {
                let mut m = s.serialize_map(Some(2))?;
                m.serialize_entry("result", t.tool_result(result))?;
                m.serialize_entry("success", &success)?;
                m.end()
            }
            (Kind::Respond { .. }, false) => s.serialize_map(Some(0))?.end(),
            (Kind::Respond { content, turns }, true) => {
                let mut m = s.serialize_map(Some(2))?;
                m.serialize_entry("content", t.s(content))?;
                m.serialize_entry("turns", &turns)?;
                m.end()
            }
        }
    }
}

struct Calls<'c>(TraceView<'c>, (usize, usize));

impl Serialize for Calls<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let t = self.0;
        let calls = t.trace.calls.get(self.1.0..self.1.1).unwrap_or(&[]);
        let mut out = s.serialize_seq(Some(calls.len()))?;
        for &(name, arguments) in calls {
            out.serialize_element(&Call(t.s(name), t.s(arguments)))?;
        }
        out.end()
    }
}

struct Call<'c>(&'c str, &'c str);

impl Serialize for Call<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(2))?;
        m.serialize_entry("arguments", self.1)?;
        m.serialize_entry("name", self.0)?;
        m.end()
    }
}
