//! A model's output, as arbitrary token bytes split into arbitrary pieces, through the
//! path a session runs: UTF-8 reassembly, `<think>` filtering, tool-call extraction and
//! parsing in both Qwen formats. It never panics, and where the tokens happen to split
//! the text changes nothing: streamed in pieces, the output yields exactly the answer
//! and calls it yields read whole, since a tokenizer can split a tag anywhere.
//!
//! Calls come only from complete `<tool_call>` blocks, and the visible answer is never
//! parsed for calls. Markup the model writes outside a block stays plain text, even
//! where removing a block joins two fragments into a tag (`<to` + block + `ol_call>`).
//!
//! Input: one byte of flags (bit 0: the Qwen3.5 format, bit 1: thinking), one byte of
//! piece length (0: one piece), then the output itself, kept contiguous so tags from
//! the dictionary and seeds stay whole.

#![no_main]

use libfuzzer_sys::fuzz_target;
use pr3_llamacpp::fuzzing::parse_output;
use pr3_llamacpp::{PromptFormat, ToolDef};

fuzz_target!(|data: &[u8]| {
    let [flags, piece, output @ ..] = data else {
        return;
    };
    let format = if flags & 1 == 1 {
        PromptFormat::Qwen35 {
            preserve_thinking: false,
        }
    } else {
        PromptFormat::Qwen3
    };
    let thinking = flags & 2 == 2;
    let pieces: Vec<Vec<u8>> = match usize::from(*piece) {
        0 => vec![output.to_vec()],
        n => output.chunks(n).map(<[u8]>::to_vec).collect(),
    };
    let tools = [ToolDef {
        name: "lookup".to_string(),
        description: "Looks something up".to_string(),
        parameters: r#"{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"}},"required":["query"]}"#.to_string(),
    }];
    let streamed = parse_output(&pieces, format, thinking, &tools);
    let whole = parse_output(&[output.to_vec()], format, thinking, &tools);
    assert_eq!(
        streamed, whole,
        "the result depends on where the pieces split"
    );
});
