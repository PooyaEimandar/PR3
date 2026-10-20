//! Talk to a GGUF model from the terminal, streaming the answer.
//!
//! cargo run -p pr3-llamacpp --release --example chat -- models/Qwen3-0.6B-Q8_0.gguf "Hello"

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use pr3_llamacpp::{ChatMessage, ChatRole, Engine, EngineConfig};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(path), Some(prompt)) = (args.next(), args.next()) else {
        eprintln!("usage: chat <model.gguf> <prompt>");
        return ExitCode::FAILURE;
    };
    let config = EngineConfig {
        gpu_layers: if cfg!(target_os = "macos") { 999 } else { 0 },
        ..EngineConfig::default()
    };

    let started = Instant::now();
    let engine = match Engine::load(&PathBuf::from(path), config) {
        Ok(engine) => engine,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!("[loaded {} in {:.2?}]", engine.label(), started.elapsed());

    let messages = [ChatMessage {
        role: ChatRole::User,
        content: prompt,
    }];
    let started = Instant::now();
    let mut stdout = std::io::stdout();
    let result = engine.chat(&messages, &mut |piece| {
        let _ = write!(stdout, "{piece}");
        let _ = stdout.flush();
    });
    println!();
    match result {
        Ok(answer) => {
            eprintln!("[{} chars in {:.2?}]", answer.len(), started.elapsed());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
