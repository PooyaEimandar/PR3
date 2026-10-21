//! Tool calling in the prompt: how tools are described to the model and how its
//! `<tool_call>` blocks are read back. Each Qwen family has its own wording, which
//! follows that family's chat template so the model sees what it was trained on:
//! [`hermes`] for Qwen3, [`xml`] for Qwen3.5 and later.

pub(crate) mod hermes;
pub(crate) mod xml;

use serde_json::Value;

/// A tool the model may call, described for the prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON Schema for the arguments object, as text.
    pub parameters: String,
}

/// A tool call as written by the model: untrusted text, validated by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallText {
    pub name: String,
    /// A JSON object as text, or the raw block when it could not be read.
    pub arguments: String,
}

pub(crate) const CALL_OPEN: &str = "<tool_call>";
pub(crate) const CALL_CLOSE: &str = "</tool_call>";

/// A block that is not a tool call in any known form: an empty name and the raw text,
/// so the caller can refuse it and tell the model why.
pub(crate) fn malformed(raw: &str) -> ToolCallText {
    ToolCallText {
        name: String::new(),
        arguments: raw.to_string(),
    }
}

/// One tool as the templates' `tool | tojson` writes it.
pub(crate) fn tool_json(tool: &ToolDef) -> String {
    let parameters = if serde_json::from_str::<Value>(&tool.parameters).is_ok() {
        python_json(&tool.parameters)
    } else {
        r#"{"type": "object"}"#.to_string()
    };
    format!(
        r#"{{"type": "function", "function": {{"name": {}, "description": {}, "parameters": {parameters}}}}}"#,
        json_string(&tool.name),
        json_string(&tool.description),
    )
}

pub(crate) fn json_string(s: &str) -> String {
    Value::String(s.to_string()).to_string()
}

/// Re-spaces JSON text the way Python's `json.dumps` writes it (`", "` and `": "`), which
/// is how the chat templates render tools and arguments. Key order is kept.
pub(crate) fn python_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    let mut in_string = false;
    let mut escaped = false;
    for c in text.chars() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            ':' | ',' => {
                out.push(c);
                out.push(' ');
            }
            c if c.is_whitespace() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_json_spacing_keeps_strings_and_order() {
        assert_eq!(
            python_json(r#"{"b":1,"a":[1, 2],"s":"x: y, \"z\""}"#),
            r#"{"b": 1, "a": [1, 2], "s": "x: y, \"z\""}"#
        );
    }

    #[test]
    fn tool_json_matches_the_template_layout() {
        let tool = ToolDef {
            name: "get_time".to_string(),
            description: "Current time".to_string(),
            parameters: r#"{"type":"object","properties":{}}"#.to_string(),
        };
        assert_eq!(
            tool_json(&tool),
            r#"{"type": "function", "function": {"name": "get_time", "description": "Current time", "parameters": {"type": "object", "properties": {}}}}"#
        );
    }
}
