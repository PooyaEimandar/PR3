//! Qwen3.5-family tool calling (Qwen3.5, 3.6, 3.8): XML calls, one `<parameter=…>` per
//! argument, with values written raw rather than as JSON.
//!
//! ```text
//! <tool_call>
//! <function=calculate>
//! <parameter=expression>
//! 1234 * 5678
//! </parameter>
//! </function>
//! </tool_call>
//! ```
//!
//! Reading follows vLLM's `qwen3_coder` parser, which Qwen's model cards point to: one
//! newline is trimmed from each side of a value, values are converted by the type the
//! tool's schema gives the parameter, and a missing `</parameter>` is tolerated.

use serde_json::{Map, Number, Value};

use super::{CALL_CLOSE, CALL_OPEN, ToolCallText, ToolDef, hermes, malformed, tool_json};

const FUNCTION_OPEN: &str = "<function=";
const FUNCTION_CLOSE: &str = "</function>";
const PARAMETER_OPEN: &str = "<parameter=";
const PARAMETER_CLOSE: &str = "</parameter>";

/// Verbatim from the Qwen3.5 and Qwen3.8 chat templates.
const INSTRUCTIONS: &str = "\n\nIf you choose to call a function ONLY reply in the following \
format with NO suffix:\n\n<tool_call>\n<function=example_function_name>\n\
<parameter=example_parameter_1>\nvalue_1\n</parameter>\n<parameter=example_parameter_2>\n\
This is the value for the second parameter\nthat can span\nmultiple lines\n</parameter>\n\
</function>\n</tool_call>\n\n<IMPORTANT>\nReminder:\n- Function calls MUST follow the \
specified format: an inner <function=...></function> block must be nested within \
<tool_call></tool_call> XML tags\n- Required parameters MUST be specified\n- You may provide \
optional reasoning for your function call in natural language BEFORE the function call, but \
NOT after\n- If there is no function call available, answer the question like normal with \
your current knowledge and do not tell the user about function calls\n</IMPORTANT>";

/// The system-prompt section that lists the tools. It comes before the system prompt.
pub(crate) fn tools_section(tools: &[ToolDef]) -> String {
    let mut s = String::from("# Tools\n\nYou have access to the following functions:\n\n<tools>");
    for tool in tools {
        s.push('\n');
        s.push_str(&tool_json(tool));
    }
    s.push_str("\n</tools>");
    s.push_str(INSTRUCTIONS);
    s
}

/// Assistant text followed by its tool calls, as the model writes them.
pub(crate) fn render_calls(content: &str, calls: &[ToolCallText]) -> String {
    let mut s = content.to_string();
    for (i, call) in calls.iter().enumerate() {
        if i > 0 {
            s.push('\n');
        } else if !content.trim().is_empty() {
            s.push_str("\n\n");
        }
        s.push_str(&format!("{CALL_OPEN}\n{FUNCTION_OPEN}{}>\n", call.name));
        if let Ok(Value::Object(arguments)) = serde_json::from_str::<Value>(&call.arguments) {
            for (key, value) in &arguments {
                let value = match value {
                    Value::String(text) => text.clone(),
                    other => super::python_json(&other.to_string()),
                };
                s.push_str(&format!(
                    "{PARAMETER_OPEN}{key}>\n{value}\n{PARAMETER_CLOSE}\n"
                ));
            }
        }
        s.push_str(&format!("{FUNCTION_CLOSE}\n{CALL_CLOSE}"));
    }
    s
}

/// Reads one `<tool_call>` block, using `tools` to type the arguments.
pub(crate) fn parse_call(block: &str, tools: &[ToolDef]) -> ToolCallText {
    let raw = block.trim();
    let Some((_, after_open)) = raw.split_once(FUNCTION_OPEN) else {
        // A reply in the older JSON form is still a call.
        return if raw.starts_with('{') {
            hermes::parse_call(raw)
        } else {
            malformed(raw)
        };
    };
    let Some((name, body)) = after_open.split_once('>') else {
        return malformed(raw);
    };
    let name = name.trim();
    let body = body
        .split_once(FUNCTION_CLOSE)
        .map_or(body, |(inside, _)| inside);
    let schema = properties(tools, name);

    let mut arguments = Map::new();
    let mut rest = body;
    while let Some((_, after)) = rest.split_once(PARAMETER_OPEN) {
        let Some((key, tail)) = after.split_once('>') else {
            break;
        };
        // A value ends at its closing tag or, if that is missing, at the next parameter.
        let end = [tail.find(PARAMETER_CLOSE), tail.find(PARAMETER_OPEN)]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(tail.len());
        let (value, remaining) = tail.split_at(end);
        let value = value.strip_prefix('\n').unwrap_or(value);
        let value = value.strip_suffix('\n').unwrap_or(value);
        let key = key.trim();
        let declared = schema.as_ref().and_then(|s| s.get(key));
        arguments.insert(key.to_string(), convert(value, declared));
        rest = remaining;
    }
    ToolCallText {
        name: name.to_string(),
        arguments: Value::Object(arguments).to_string(),
    }
}

/// The `properties` of the named tool's schema.
fn properties(tools: &[ToolDef], name: &str) -> Option<Map<String, Value>> {
    let tool = tools.iter().find(|t| t.name == name)?;
    match serde_json::from_str::<Value>(&tool.parameters).ok()? {
        Value::Object(mut schema) => match schema.remove("properties")? {
            Value::Object(properties) => Some(properties),
            _ => None,
        },
        _ => None,
    }
}

/// Turns a raw value into JSON by its declared type. A value that does not fit stays a
/// string, so validation can tell the model what was wrong; an undeclared parameter
/// stays a string for the same reason.
fn convert(value: &str, declared: Option<&Value>) -> Value {
    let Some(declared) = declared else {
        return Value::String(value.to_string());
    };
    if value.eq_ignore_ascii_case("null") {
        return Value::Null;
    }
    let trimmed = value.trim();
    let converted = match type_name(declared) {
        Some("string") => None,
        Some("integer") => trimmed.parse::<i64>().ok().map(Value::from),
        Some("number") => trimmed.parse::<f64>().ok().and_then(number),
        Some("boolean") => match trimmed.to_ascii_lowercase().as_str() {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
        // Objects, arrays and untyped parameters are written as JSON.
        _ => serde_json::from_str(trimmed).ok(),
    };
    converted.unwrap_or_else(|| Value::String(value.to_string()))
}

fn type_name(schema: &Value) -> Option<&str> {
    match schema.get("type")? {
        Value::String(t) => Some(t),
        Value::Array(types) => types
            .iter()
            .filter_map(Value::as_str)
            .find(|t| *t != "null"),
        _ => None,
    }
}

/// Whole numbers become integers, as vLLM does.
fn number(f: f64) -> Option<Value> {
    if f.fract() == 0.0 && f.abs() < 9_007_199_254_740_992.0 {
        Some(Value::from(f as i64))
    } else {
        Number::from_f64(f).map(Value::Number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, parameters: &str) -> ToolDef {
        ToolDef {
            name: name.to_string(),
            description: String::new(),
            parameters: parameters.to_string(),
        }
    }

    fn tools() -> Vec<ToolDef> {
        vec![
            tool(
                "calculate",
                r#"{"type":"object","properties":{"expression":{"type":"string"}}}"#,
            ),
            tool(
                "remind",
                r#"{"type":"object","properties":{"minutes":{"type":"integer"},"loud":{"type":"boolean"},"tags":{"type":"array"},"note":{"type":["string","null"]}}}"#,
            ),
            tool("get_time", r#"{"type":"object","properties":{}}"#),
        ]
    }

    #[test]
    fn tools_section_matches_the_template() {
        let s = tools_section(&tools()[..1]);
        assert!(s.starts_with(
            "# Tools\n\nYou have access to the following functions:\n\n<tools>\n\
             {\"type\": \"function\", \"function\": {\"name\": \"calculate\""
        ));
        assert!(s.contains("\n</tools>\n\nIf you choose to call a function ONLY reply"));
        assert!(s.ends_with("do not tell the user about function calls\n</IMPORTANT>"));
    }

    #[test]
    fn parses_a_call_as_qwen_writes_it() {
        let block = "\n<function=calculate>\n<parameter=expression>\n1234 * 5678\n</parameter>\n</function>\n";
        let call = parse_call(block, &tools());
        assert_eq!(call.name, "calculate");
        assert_eq!(call.arguments, r#"{"expression":"1234 * 5678"}"#);
    }

    #[test]
    fn multi_line_values_keep_their_inner_newlines() {
        let block =
            "<function=calculate>\n<parameter=expression>\n1 +\n2\n\n</parameter>\n</function>";
        let call = parse_call(block, &tools());
        assert_eq!(call.arguments, r#"{"expression":"1 +\n2\n"}"#);
    }

    #[test]
    fn values_are_typed_by_the_schema() {
        let block = "<function=remind>\n<parameter=minutes>\n15\n</parameter>\n\
                     <parameter=loud>\nTrue\n</parameter>\n<parameter=tags>\n[\"a\", \"b\"]\n</parameter>\n\
                     <parameter=note>\nnull\n</parameter>\n</function>";
        let call = parse_call(block, &tools());
        assert_eq!(
            call.arguments,
            r#"{"loud":true,"minutes":15,"note":null,"tags":["a","b"]}"#
        );
    }

    #[test]
    fn values_that_do_not_fit_stay_strings_for_validation() {
        let block = "<function=remind>\n<parameter=minutes>\nsoon\n</parameter>\n\
                     <parameter=shell>\nrm -rf /\n</parameter>\n</function>";
        let call = parse_call(block, &tools());
        assert_eq!(call.arguments, r#"{"minutes":"soon","shell":"rm -rf /"}"#);
    }

    #[test]
    fn tolerates_missing_closing_tags_and_no_arguments() {
        let block = "<function=remind>\n<parameter=minutes>\n5\n<parameter=loud>\nfalse";
        assert_eq!(
            parse_call(block, &tools()).arguments,
            r#"{"loud":false,"minutes":5}"#
        );
        let call = parse_call("\n<function=get_time>\n</function>\n", &tools());
        assert_eq!(
            (call.name.as_str(), call.arguments.as_str()),
            ("get_time", "{}")
        );
    }

    #[test]
    fn json_form_and_garbage() {
        let call = parse_call(r#"{"name": "get_time", "arguments": {}}"#, &tools());
        assert_eq!(call.name, "get_time");
        assert_eq!(parse_call("calculate(2+2)", &tools()).name, "");
    }

    #[test]
    fn rendered_calls_match_the_template_and_parse_back() {
        let first = ToolCallText {
            name: "calculate".to_string(),
            arguments: r#"{"expression":"2+2"}"#.to_string(),
        };
        let second = ToolCallText {
            name: "remind".to_string(),
            arguments: r#"{"loud":true,"minutes":5}"#.to_string(),
        };
        let text = render_calls("Checking.", &[first.clone(), second.clone()]);
        assert_eq!(
            text,
            "Checking.\n\n<tool_call>\n<function=calculate>\n<parameter=expression>\n2+2\n\
             </parameter>\n</function>\n</tool_call>\n<tool_call>\n<function=remind>\n\
             <parameter=loud>\ntrue\n</parameter>\n<parameter=minutes>\n5\n</parameter>\n\
             </function>\n</tool_call>"
        );
        let blocks: Vec<&str> = text
            .split(CALL_OPEN)
            .skip(1)
            .filter_map(|s| s.split(CALL_CLOSE).next())
            .collect();
        assert_eq!(parse_call(blocks[0], &tools()), first);
        assert_eq!(parse_call(blocks[1], &tools()), second);
        assert!(render_calls("", &[first]).starts_with(CALL_OPEN));
    }
}
