//! Writes a conversation into the prompt exactly as each Qwen family's chat template
//! does, so the model sees what it was trained on. PR3 renders the prompt itself because
//! llama.cpp's built-in template engine cannot render tools.

use crate::tools::{ToolCallText, ToolDef, hermes, xml};
use crate::{ChatMessage, ChatRole};

/// How a model family writes tools, tool calls and reasoning into its prompt. Read from
/// the chat template inside the GGUF file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptFormat {
    /// Qwen3: tool calls are JSON objects inside `<tool_call>` tags.
    Qwen3,
    /// Qwen3.5 and later (3.6, 3.8): tool calls are XML, `<function=…>` with one
    /// `<parameter=…>` per argument, and the tools come before the system prompt.
    Qwen35 {
        /// Every earlier assistant turn keeps its (here empty) reasoning block, as
        /// Qwen3.8's template does by default. Qwen3.5 keeps it only within the turn
        /// being answered.
        preserve_thinking: bool,
    },
}

/// The reasoning block a non-thinking answer starts with: the templates' own
/// `enable_thinking=False`.
const EMPTY_THINK: &str = "<think>\n\n</think>\n\n";

impl PromptFormat {
    /// Recognises the format from a GGUF's embedded chat template, or `None` for a model
    /// family PR3 does not support.
    pub fn detect(template: &str) -> Option<Self> {
        if !template.contains("<|im_start|>") {
            return None;
        }
        if template.contains("<function=") {
            Some(Self::Qwen35 {
                preserve_thinking: template.contains("preserve_thinking"),
            })
        } else if template.contains("<tool_call>") {
            Some(Self::Qwen3)
        } else {
            None
        }
    }

    pub(crate) fn parse_call(self, block: &str, tools: &[ToolDef]) -> ToolCallText {
        match self {
            Self::Qwen3 => hermes::parse_call(block),
            Self::Qwen35 { .. } => xml::parse_call(block, tools),
        }
    }

    /// The prompt itself opens the reasoning block, so the model's output starts inside it.
    pub(crate) fn opens_think(self, thinking: bool) -> bool {
        thinking && matches!(self, Self::Qwen35 { .. })
    }

    /// Answers are written back without the reasoning block they were generated after
    /// once a new question follows, so the next turn's prompt differs from what was
    /// generated, starting right after the question.
    pub(crate) fn rewrites_history(self) -> bool {
        matches!(
            self,
            Self::Qwen35 {
                preserve_thinking: false
            }
        )
    }

    fn tools_section(self, tools: &[ToolDef]) -> String {
        match self {
            Self::Qwen3 => hermes::tools_section(tools),
            Self::Qwen35 { .. } => xml::tools_section(tools),
        }
    }

    fn render_calls(self, content: &str, calls: &[ToolCallText]) -> String {
        match self {
            Self::Qwen3 => hermes::render_calls(content, calls),
            Self::Qwen35 { .. } => xml::render_calls(content, calls),
        }
    }

    fn system_with_tools(self, system: Option<&str>, tools: &[ToolDef]) -> Option<String> {
        if tools.is_empty() {
            return system.map(str::to_string);
        }
        let section = self.tools_section(tools);
        Some(match (self, system) {
            (Self::Qwen3, Some(system)) => format!("{system}\n\n{section}"),
            (Self::Qwen35 { .. }, Some(system)) if !system.is_empty() => {
                format!("{section}\n\n{system}")
            }
            _ => section,
        })
    }
}

/// A rendered prompt and where its turns that later prompts share end.
pub(crate) struct Rendered {
    /// The whole prompt, ending where the model starts its answer.
    pub text: String,
    /// Byte offset just after the system turn.
    pub system_end: Option<usize>,
    /// Byte offset just after the last user question.
    pub query_end: Option<usize>,
}

pub(crate) fn render(
    format: PromptFormat,
    messages: &[&ChatMessage],
    tools: &[ToolDef],
    thinking: bool,
) -> Rendered {
    let (system, rest) = match messages.split_first() {
        Some((first, rest)) if first.role == ChatRole::System => (Some(first.content.trim()), rest),
        _ => (None, messages),
    };
    let mut out = String::new();
    let mut system_end = None;
    let mut query_end = None;
    if let Some(system) = format.system_with_tools(system, tools) {
        turn(&mut out, "system", &system);
        system_end = Some(out.len());
    }

    let last_query = rest.iter().rposition(|m| m.role == ChatRole::User);
    for (i, m) in rest.iter().enumerate() {
        match m.role {
            ChatRole::System => turn(&mut out, "system", m.content.trim()),
            ChatRole::User => {
                turn(&mut out, "user", m.content.trim());
                if last_query == Some(i) {
                    query_end = Some(out.len());
                }
            }
            ChatRole::Assistant => {
                let mut text = String::new();
                if let PromptFormat::Qwen35 { preserve_thinking } = format {
                    if preserve_thinking || last_query.is_none_or(|q| i > q) {
                        text.push_str(EMPTY_THINK);
                    }
                }
                text.push_str(&format.render_calls(m.content.trim(), &m.tool_calls));
                turn(&mut out, "assistant", &text);
            }
            // A run of tool results is one user turn of <tool_response> blocks.
            ChatRole::Tool => {
                let after_tool = i
                    .checked_sub(1)
                    .and_then(|p| rest.get(p))
                    .is_some_and(|p| p.role == ChatRole::Tool);
                if !after_tool {
                    out.push_str("<|im_start|>user");
                }
                out.push_str("\n<tool_response>\n");
                out.push_str(m.content.trim());
                out.push_str("\n</tool_response>");
                if !rest.get(i + 1).is_some_and(|n| n.role == ChatRole::Tool) {
                    out.push_str("<|im_end|>\n");
                }
            }
        }
    }

    out.push_str("<|im_start|>assistant\n");
    out.push_str(match (format, thinking) {
        (_, false) => EMPTY_THINK,
        (PromptFormat::Qwen3, true) => "",
        (PromptFormat::Qwen35 { .. }, true) => "<think>\n",
    });
    Rendered {
        text: out,
        system_end,
        query_end,
    }
}

fn turn(out: &mut String, role: &str, content: &str) {
    out.push_str("<|im_start|>");
    out.push_str(role);
    out.push('\n');
    out.push_str(content);
    out.push_str("<|im_end|>\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    const QWEN35: PromptFormat = PromptFormat::Qwen35 {
        preserve_thinking: false,
    };
    const QWEN38: PromptFormat = PromptFormat::Qwen35 {
        preserve_thinking: true,
    };

    fn calc() -> ToolDef {
        ToolDef {
            name: "calculate".to_string(),
            description: "Evaluate arithmetic".to_string(),
            parameters: r#"{"type":"object","properties":{"expression":{"type":"string"}},"required":["expression"]}"#.to_string(),
        }
    }

    fn msg(role: ChatRole, content: &str) -> ChatMessage {
        ChatMessage::new(role, content)
    }

    fn calling(content: &str, expression: &str) -> ChatMessage {
        ChatMessage {
            tool_calls: vec![ToolCallText {
                name: "calculate".to_string(),
                arguments: format!(r#"{{"expression":"{expression}"}}"#),
            }],
            ..msg(ChatRole::Assistant, content)
        }
    }

    /// system, user, a tool call and its result.
    fn tool_turn() -> Vec<ChatMessage> {
        vec![
            msg(ChatRole::System, "You are PR3."),
            msg(ChatRole::User, "What is 2+2?"),
            calling("", "2+2"),
            msg(ChatRole::Tool, "4"),
        ]
    }

    fn render_all(format: PromptFormat, messages: &[ChatMessage], tools: &[ToolDef]) -> String {
        let refs: Vec<&ChatMessage> = messages.iter().collect();
        render(format, &refs, tools, false).text
    }

    #[test]
    fn detects_the_family_from_the_template() {
        assert_eq!(
            PromptFormat::detect("<|im_start|>{{ '<tool_call>' }}"),
            Some(PromptFormat::Qwen3)
        );
        assert_eq!(
            PromptFormat::detect("<|im_start|><tool_call>\n<function="),
            Some(QWEN35)
        );
        assert_eq!(
            PromptFormat::detect("<|im_start|><function= preserve_thinking"),
            Some(QWEN38)
        );
        assert_eq!(PromptFormat::detect("[INST] {{ messages }}"), None);
    }

    /// Expected text follows Qwen3.5's chat_template.jinja for the same conversation.
    #[test]
    fn qwen35_tool_turn_matches_the_template() {
        let prompt = render_all(QWEN35, &tool_turn(), &[calc()]);
        let (head, tail) = prompt.split_once("</IMPORTANT>").expect("instructions");
        assert_eq!(
            head.lines().take(6).collect::<Vec<_>>(),
            [
                "<|im_start|>system",
                "# Tools",
                "",
                "You have access to the following functions:",
                "",
                "<tools>"
            ]
        );
        assert!(head.contains(
            r#"{"type": "function", "function": {"name": "calculate", "description": "Evaluate arithmetic", "parameters": {"type": "object", "properties": {"expression": {"type": "string"}}, "required": ["expression"]}}}"#
        ));
        assert_eq!(
            tail,
            "\n\nYou are PR3.<|im_end|>\n\
             <|im_start|>user\nWhat is 2+2?<|im_end|>\n\
             <|im_start|>assistant\n<think>\n\n</think>\n\n<tool_call>\n<function=calculate>\n\
             <parameter=expression>\n2+2\n</parameter>\n</function>\n</tool_call><|im_end|>\n\
             <|im_start|>user\n<tool_response>\n4\n</tool_response><|im_end|>\n\
             <|im_start|>assistant\n<think>\n\n</think>\n\n"
        );
    }

    /// Expected text follows Qwen3's chat template for the same conversation.
    #[test]
    fn qwen3_tool_turn_matches_the_template() {
        let prompt = render_all(PromptFormat::Qwen3, &tool_turn(), &[calc()]);
        assert!(prompt.starts_with("<|im_start|>system\nYou are PR3.\n\n# Tools\n\nYou may call"));
        let (_, tail) = prompt
            .split_once("\n</tool_call><|im_end|>\n")
            .expect("system turn");
        assert_eq!(
            tail,
            "<|im_start|>user\nWhat is 2+2?<|im_end|>\n\
             <|im_start|>assistant\n<tool_call>\n{\"name\": \"calculate\", \"arguments\": \
             {\"expression\": \"2+2\"}}\n</tool_call><|im_end|>\n\
             <|im_start|>user\n<tool_response>\n4\n</tool_response><|im_end|>\n\
             <|im_start|>assistant\n<think>\n\n</think>\n\n"
        );
    }

    #[test]
    fn earlier_turns_keep_reasoning_blocks_only_in_qwen38() {
        let mut messages = tool_turn();
        messages.push(msg(ChatRole::Assistant, "4."));
        messages.push(msg(ChatRole::User, "Thanks"));
        let earlier = "<|im_start|>assistant\n<think>\n\n</think>\n\n<tool_call>";
        assert!(!render_all(QWEN35, &messages, &[calc()]).contains(earlier));
        assert!(render_all(QWEN38, &messages, &[calc()]).contains(earlier));
        assert!(
            render_all(QWEN38, &messages, &[calc()])
                .contains("<|im_start|>assistant\n<think>\n\n</think>\n\n4.<|im_end|>")
        );
    }

    #[test]
    fn consecutive_tool_results_share_one_user_turn() {
        let mut messages = tool_turn();
        messages.push(msg(ChatRole::Tool, "5"));
        let prompt = render_all(QWEN35, &messages, &[calc()]);
        assert!(prompt.contains(
            "<|im_start|>user\n<tool_response>\n4\n</tool_response>\n<tool_response>\n5\n\
             </tool_response><|im_end|>\n"
        ));
    }

    #[test]
    fn no_tools_and_thinking() {
        let messages = [msg(ChatRole::User, "Hi")];
        let refs: Vec<&ChatMessage> = messages.iter().collect();
        assert_eq!(
            render(QWEN35, &refs, &[], false).text,
            "<|im_start|>user\nHi<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n"
        );
        assert!(
            render(QWEN35, &refs, &[], true)
                .text
                .ends_with("<|im_start|>assistant\n<think>\n")
        );
        assert!(
            render(PromptFormat::Qwen3, &refs, &[], true)
                .text
                .ends_with("<|im_start|>assistant\n")
        );
        assert!(QWEN35.opens_think(true) && !PromptFormat::Qwen3.opens_think(true));
    }

    #[test]
    fn boundaries_mark_the_system_turn_and_the_last_question() {
        let mut messages = tool_turn();
        messages.push(msg(ChatRole::Assistant, "4."));
        messages.push(msg(ChatRole::User, "Thanks"));
        let refs: Vec<&ChatMessage> = messages.iter().collect();
        let r = render(QWEN35, &refs, &[calc()], false);
        let system_end = r.system_end.expect("system turn");
        let query_end = r.query_end.expect("question");
        assert!(r.text[..system_end].ends_with("You are PR3.<|im_end|>\n"));
        assert!(r.text[..query_end].ends_with("<|im_start|>user\nThanks<|im_end|>\n"));
        assert_eq!(
            &r.text[query_end..],
            "<|im_start|>assistant\n<think>\n\n</think>\n\n"
        );

        let only_user = [msg(ChatRole::User, "Hi")];
        let refs: Vec<&ChatMessage> = only_user.iter().collect();
        let r = render(QWEN35, &refs, &[], false);
        let question = "<|im_start|>user\nHi<|im_end|>\n".len();
        assert_eq!((r.system_end, r.query_end), (None, Some(question)));
        assert!(QWEN35.rewrites_history() && !QWEN38.rewrites_history());
        assert!(!PromptFormat::Qwen3.rewrites_history());
    }
}
