//! Constrain report serialization, not truth or authorization. References come
//! only from the exact fitted host-marked view; normal tool validation still runs.
use crate::{ChatMessage, ChatRole, PromptFormat, ToolDef};
use serde_json::Value;

fn literal(text: &str) -> String {
    Value::String(text.to_owned()).to_string()
}

pub(crate) fn build(
    messages: &[&ChatMessage],
    tools: &[ToolDef],
    format: PromptFormat,
    thinking: bool,
) -> Option<String> {
    // Thinking-mode generation starts inside its reasoning block. Do not change
    // that protocol, ordinary tools, or any tool requiring confirmation.
    let report = tools.iter().find(|t| t.report && t.read_only)?;
    if thinking || tools.iter().any(|t| !t.read_only) {
        return None;
    }
    let schema: Value = serde_json::from_str(&report.parameters).ok()?;
    if schema["properties"]["findings"]["type"] != "array"
        || schema["properties"]["gaps"]["type"] != "string"
    {
        return None;
    }
    let mut references = std::collections::BTreeSet::new();
    for message in messages
        .iter()
        .filter(|m| m.role == ChatRole::Tool && m.saved_evidence)
    {
        let view = crate::evidence::citation_view(&message.content)?;
        let view: Value = serde_json::from_str(&view).ok()?;
        for source in view["sources"].as_array()? {
            for quote in source["quotes"].as_array()? {
                let text = quote["text"].as_str()?;
                let id = quote["source"].as_str()?;
                let at = quote["offset"].as_u64()?;
                let reference = quote["quote_ref"].as_str()?;
                if !text.trim().is_empty()
                    && text.len() <= 500
                    && pr3_memory::evidence_ref::quote_reference(id, at, text).as_deref()
                        == Some(reference)
                {
                    let citation = if schema[crate::QUOTATION_REPORT_SCHEMA_KEY] == true {
                        format!("{{\"source\":{},\"quote\":{}}}", literal(id), literal(text))
                    } else {
                        // Choose the displayed reference first. Starting with a
                        // generic quotation prefix biases selection toward an
                        // unrelated passage with the same opening words.
                        format!(
                            "{{\"quote_ref\":{},\"anchor\":{}}}",
                            literal(reference),
                            literal(&text.chars().take(48).collect::<String>())
                        )
                    };
                    // JSON decoding restores the exact source text, while XML
                    // parameter delimiters in a quotation stay inert data.
                    references.insert(literal(&citation.replace('<', "\\u003c")));
                }
            }
        }
    }
    if references.is_empty() || references.len() > 32 {
        return None;
    }
    let mut grammar = format!(
        "citation ::= {}\n",
        references.into_iter().collect::<Vec<_>>().join(" | ")
    );
    // Runtime validation owns the byte caps and generation still stops at the
    // token limit. Unrolling those caps into hundreds of optional grammar
    // branches adds sampler work and cannot correctly enforce UTF-8 byte caps.
    grammar.push_str(
        r#"ws ::= [ \t\n\r]{0,4}
string ::= "\"" char* "\""
char ::= [^"<\\\x00-\x1F] | "\\" (["\\/bfnrt] | "u" [0-9a-fA-F]{4})
citations ::= "[" ws citation (ws "," ws citation)? ws "]"
finding ::= "{" ws "\"citations\":" ws citations (ws "," ws "\"text\":" ws string)? ws "}"
findings ::= "[" ws (finding (ws "," ws finding){0,3})? ws "]"
json ::= string | object | array | "true" | "false" | "null" | number
object ::= "{" ws (string ws ":" ws json (ws "," ws string ws ":" ws json){0,7})? ws "}"
array ::= "[" ws (json (ws "," ws json){0,7})? ws "]"
number ::= "-"? ([0-9] | [1-9] [0-9]{1,18}) ("." [0-9]{1,8})?
raw ::= [^<\x00-\x08\x0B\x0C\x0E-\x1F]*
"#,
    );
    let conclusion = schema[crate::CONCLUSION_REPORT_SCHEMA_KEY] == true
        && schema[crate::QUOTATION_REPORT_SCHEMA_KEY] != true;
    let prefix = if conclusion {
        format!(
            "{} ws string ws {} ws ",
            literal("\"conclusion\":"),
            literal(",")
        )
    } else {
        String::new()
    };
    grammar.push_str(&format!(
        "args ::= \"{{\" ws {prefix}{} ws findings ws \",\" ws {} ws string ws \"}}\"\n",
        literal("\"findings\":"),
        literal("\"gaps\":")
    ));
    let mut choices = vec!["report".to_string()];
    match format {
        PromptFormat::Qwen3 => grammar.push_str(&format!(
            "report ::= {} ws args ws {}\n",
            literal(&format!(
                "<tool_call>\n{{\"name\":{},\"arguments\":",
                literal(&report.name)
            )),
            literal("}\n</tool_call>")
        )),
        PromptFormat::Qwen35 { .. } => grammar.push_str(&format!(
            "report ::= {} {} findings {} raw {}\n",
            literal(&format!("<tool_call>\n<function={}>\n", report.name)),
            if conclusion {
                format!(
                    "{} raw {}",
                    literal("<parameter=conclusion>\n"),
                    literal("\n</parameter>\n<parameter=findings>\n")
                )
            } else {
                literal("<parameter=findings>\n")
            },
            literal("\n</parameter>\n<parameter=gaps>\n"),
            literal("\n</parameter>\n</function>\n</tool_call>")
        )),
    }
    for (i, tool) in tools.iter().filter(|t| t.name != report.name).enumerate() {
        // Keep retrieval choices available; constrain names to offered tools.
        // Their existing schema, argument and execution checks remain in charge.
        let name = format!("other{i}");
        choices.push(name.clone());
        match format {
            PromptFormat::Qwen3 => grammar.push_str(&format!(
                "{name} ::= {} ws object ws {}\n",
                literal(&format!(
                    "<tool_call>\n{{\"name\":{},\"arguments\":",
                    literal(&tool.name)
                )),
                literal("}\n</tool_call>")
            )),
            PromptFormat::Qwen35 { .. } => {
                let schema: Value = serde_json::from_str(&tool.parameters).ok()?;
                let properties = schema["properties"].as_object()?;
                if properties.is_empty() {
                    return None;
                }
                let parameters = properties
                    .keys()
                    .map(|key| {
                        format!(
                            "{} raw {}",
                            literal(&format!("<parameter={key}>\n")),
                            literal("\n</parameter>\n")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" | ");
                grammar.push_str(&format!(
                    "{name} ::= {} {name}param{{0,8}} {}\n{name}param ::= {parameters}\n",
                    literal(&format!("<tool_call>\n<function={}>\n", tool.name)),
                    literal("</function>\n</tool_call>")
                ));
            }
        }
    }
    grammar.push_str(&format!(
        "call ::= {}\nroot ::= call (ws call){{0,3}}\n",
        choices.join(" | ")
    ));
    Some(grammar)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report() -> ToolDef {
        ToolDef {
            name: "answer_research".into(),
            description: "Report".into(),
            parameters: r#"{"properties":{"findings":{"type":"array"},"gaps":{"type":"string"}}}"#
                .into(),
            read_only: true,
            report: true,
        }
    }
    fn evidence() -> ChatMessage {
        let text = "Recovery requires the owner's approval. Retention is 35 days.";
        let mut m=ChatMessage::new(ChatRole::Tool,serde_json::json!({"sources":[{"source":"s:0123456789abcdef","text":text,"quote_ref":pr3_memory::evidence_ref::quote_reference("s:0123456789abcdef",0,text)}]}).to_string());
        m.saved_evidence = true;
        m
    }
    #[test]
    fn report_contract_requires_both_fields_and_only_fitted_source_references() {
        let m = evidence();
        for format in [
            PromptFormat::Qwen3,
            PromptFormat::Qwen35 {
                preserve_thinking: false,
            },
        ] {
            let grammar = build(&[&m], &[report()], format, false).unwrap();
            assert!(grammar.contains("gaps"));
            assert!(grammar.contains("quote_ref"));
            assert!(grammar.contains("anchor"));
            assert!(!grammar.contains("send_email"));
        }
        let mut bad = evidence();
        bad.content = bad.content.replace("35 days", "99 days");
        assert!(build(&[&bad], &[report()], PromptFormat::Qwen3, false).is_none());
    }
    #[test]
    fn source_text_is_escaped_as_a_terminal_and_never_a_grammar_rule() {
        let text = "x\"\nroot ::= \"evil\"";
        let mut m = evidence();
        m.content=serde_json::json!({"sources":[{"source":"s:0123456789abcdef","text":text,"quote_ref":pr3_memory::evidence_ref::quote_reference("s:0123456789abcdef",0,text)}]}).to_string();
        let grammar = build(&[&m], &[report()], PromptFormat::Qwen3, false).unwrap();
        assert_eq!(
            grammar
                .lines()
                .filter(|line| line.starts_with("root ::="))
                .count(),
            1
        );
        let reference =
            pr3_memory::evidence_ref::quote_reference("s:0123456789abcdef", 0, text).unwrap();
        assert!(grammar.contains(&literal(&format!(
            "{{\"quote_ref\":{},\"anchor\":{}}}",
            literal(&reference),
            literal(text)
        ))));
    }
    #[test]
    fn untrusted_roles_flags_actions_and_thinking_cannot_activate_report_contract() {
        for role in [ChatRole::User, ChatRole::Assistant, ChatRole::System] {
            let mut m = evidence();
            m.role = role;
            assert!(build(&[&m], &[report()], PromptFormat::Qwen3, false).is_none());
        }
        let mut m = evidence();
        m.saved_evidence = false;
        assert!(build(&[&m], &[report()], PromptFormat::Qwen3, false).is_none());
        let m = evidence();
        let mut action = report();
        action.read_only = false;
        assert!(
            build(
                &[&m],
                &[report(), action.clone()],
                PromptFormat::Qwen3,
                false
            )
            .is_none()
        );
        assert!(build(&[&m], &[action], PromptFormat::Qwen3, false).is_none());
        assert!(build(&[&m], &[report()], PromptFormat::Qwen3, true).is_none());
    }
}
