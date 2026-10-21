//! Built-in tools PR3 offers the model. Each is deterministic, offline and read-only,
//! so none needs the trusted confirmation path (Roadmap §4.6). Tools run on the host;
//! the core only sees their specs and results.

pub mod calculator;
pub mod clock;

use pr3_core::{Timestamp, ToolSpec};
use serde_json::{Map, Value};

/// What a tool may know about the moment it runs, supplied by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolContext {
    pub now: Timestamp,
    /// The device's offset from UTC, in minutes (e.g. 210 for UTC+03:30).
    pub utc_offset_minutes: i32,
}

pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;

    /// Runs with already-validated arguments; `Err` is shown to the model as a failed
    /// call it can recover from.
    fn run(&self, arguments: &Map<String, Value>, ctx: &ToolContext) -> Result<String, String>;
}

/// The tools every PR3 host offers.
pub fn builtin() -> Vec<Box<dyn Tool>> {
    vec![Box::new(Calculator), Box::new(Clock)]
}

pub struct Calculator;

impl Tool for Calculator {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "calculate".to_string(),
            description: "Evaluate an arithmetic expression exactly. Use it for any \
                          calculation instead of doing the math yourself."
                .to_string(),
            parameters: r#"{"type":"object","properties":{"expression":{"type":"string","description":"Arithmetic using + - * / % ^ and parentheses, e.g. 12*(3+4)"}},"required":["expression"]}"#.to_string(),
            requires_confirmation: false,
            required_capabilities: Vec::new(),
        }
    }

    fn run(&self, arguments: &Map<String, Value>, _: &ToolContext) -> Result<String, String> {
        let expression = arguments
            .get("expression")
            .and_then(Value::as_str)
            .ok_or("missing 'expression'")?;
        calculator::evaluate(expression)
            .map(calculator::format_number)
            .map_err(|e| e.to_string())
    }
}

pub struct Clock;

impl Tool for Clock {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "get_time".to_string(),
            description: "Get the current local date, time and weekday. You do not know \
                          them otherwise: call this for any question about today, the date, \
                          the time or the day of the week."
                .to_string(),
            parameters: r#"{"type":"object","properties":{}}"#.to_string(),
            requires_confirmation: false,
            required_capabilities: Vec::new(),
        }
    }

    fn run(&self, _: &Map<String, Value>, ctx: &ToolContext) -> Result<String, String> {
        Ok(clock::describe(ctx.now, ctx.utc_offset_minutes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_specs_are_accepted_by_the_core() {
        let mut core = pr3_core::Core::new(pr3_core::Config::default());
        for tool in builtin() {
            core.register_tool(tool.spec()).expect("valid spec");
        }
    }

    #[test]
    fn calculator_runs_with_validated_arguments() {
        let args: Map<String, Value> =
            serde_json::from_str(r#"{"expression":"1234*5678"}"#).expect("json");
        let ctx = ToolContext {
            now: Timestamp(0),
            utc_offset_minutes: 0,
        };
        assert_eq!(Calculator.run(&args, &ctx), Ok("7006652".to_string()));
    }
}
