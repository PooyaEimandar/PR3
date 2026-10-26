//! Qwen3 (Hermes-style) tool calling: each call is a JSON object inside `<tool_call>`.

use serde_json::Value;

use super::{CALL_CLOSE, CALL_OPEN, ToolCallText, ToolDef, malformed, python_json, tool_json};

/// The system-prompt section that lists the tools.
pub(crate) fn tools_section(tools: &[ToolDef]) -> String {
    let mut s = String::from(
        "# Tools\n\nYou may call one or more functions to assist with the user query.\n\n\
         You are provided with function signatures within <tools></tools> XML tags:\n<tools>",
    );
    for tool in tools {
        s.push('\n');
        s.push_str(&tool_json(tool));
    }
    s.push_str(
        "\n</tools>\n\nFor each function call, return a json object with function name and \
         arguments within <tool_call></tool_call> XML tags:\n<tool_call>\n\
         {\"name\": <function-name>, \"arguments\": <args-json-object>}\n</tool_call>",
    );
    s
}

/// Assistant text followed by its tool calls, as the model writes them.
pub(crate) fn render_calls(content: &str, calls: &[ToolCallText]) -> String {
    let mut s = content.to_string();
    for call in calls {
        if !s.is_empty() {
            s.push('\n');
        }
        // Valid JSON arguments are embedded as an object; anything else as a string.
        let arguments = serde_json::from_str::<Value>(&call.arguments).map_or_else(
            |_| Value::String(call.arguments.clone()).to_string(),
            |v| python_json(&v.to_string()),
        );
        s.push_str(&format!(
            "{CALL_OPEN}\n{{\"name\": \"{}\", \"arguments\": {arguments}}}\n{CALL_CLOSE}",
            call.name
        ));
    }
    s
}

/// Reads one `<tool_call>` block.
pub(crate) fn parse_call(block: &str) -> ToolCallText {
    let raw = block.trim();
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Object(map)) => {
            let name = map
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let arguments = match map.get("arguments") {
                Some(Value::String(s)) => s.clone(),
                Some(v) => v.to_string(),
                None => "{}".to_string(),
            };
            ToolCallText { name, arguments }
        }
        _ => malformed(raw),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calc() -> ToolDef {
        ToolDef {
            name: "calculate".to_string(),
            description: "Evaluate arithmetic".to_string(),
            parameters: r#"{"type":"object","properties":{"expression":{"type":"string"}}}"#
                .to_string(),
        }
    }

    #[test]
    fn tools_section_lists_each_tool_as_json() {
        let s = tools_section(&[calc()]);
        assert!(s.starts_with("# Tools"));
        assert!(s.contains(r#""name": "calculate""#), "{s}");
        assert!(s.contains("<tool_call></tool_call> XML tags"));
    }

    #[test]
    fn parses_object_and_string_arguments() {
        let call = parse_call(r#" {"name": "calculate", "arguments": {"expression": "2+2"}} "#);
        assert_eq!(call.name, "calculate");
        assert_eq!(call.arguments, r#"{"expression":"2+2"}"#);
        let call = parse_call(r#"{"name":"calculate","arguments":"{\"expression\":\"1\"}"}"#);
        assert_eq!(call.arguments, r#"{"expression":"1"}"#);
        let call = parse_call(r#"{"name":"get_time"}"#);
        assert_eq!(call.arguments, "{}");
    }

    #[test]
    fn malformed_block_keeps_the_raw_text() {
        let call = parse_call("calculate(2+2)");
        assert_eq!(call.name, "");
        assert_eq!(call.arguments, "calculate(2+2)");
    }

    #[test]
    fn rendered_calls_parse_back() {
        let original = ToolCallText {
            name: "calculate".to_string(),
            arguments: r#"{"expression":"2+2"}"#.to_string(),
        };
        let text = render_calls("Let me check.", std::slice::from_ref(&original));
        assert_eq!(
            text,
            "Let me check.\n<tool_call>\n{\"name\": \"calculate\", \"arguments\": \
             {\"expression\": \"2+2\"}}\n</tool_call>"
        );
        let inner = text
            .split(CALL_OPEN)
            .nth(1)
            .and_then(|s| s.split(CALL_CLOSE).next())
            .unwrap_or_default();
        assert_eq!(parse_call(inner), original);
    }
}
