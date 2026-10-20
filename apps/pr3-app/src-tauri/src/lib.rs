//! PR3 reference app. The Rust side owns the runtime; the webview only renders.

mod host;
#[cfg(target_os = "macos")]
mod llama;

use std::sync::Arc;

use host::{Backend, Host, MockBackend};
use serde::Serialize;
use tauri::Manager;
use tauri::ipc::Channel;

#[derive(Serialize)]
struct PlatformInfo {
    os: &'static str,
    arch: &'static str,
    version: &'static str,
    model: String,
}

#[tauri::command]
fn platform_info(host: tauri::State<'_, Host>) -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        version: env!("CARGO_PKG_VERSION"),
        model: host.backend_label(),
    }
}

/// Runs one user turn through the core. Async, so Tauri runs it off the main thread;
/// visible tokens stream through `on_token` while the model runs.
#[tauri::command]
async fn send_message(
    text: String,
    on_token: Channel<String>,
    host: tauri::State<'_, Host>,
) -> Result<Vec<pr3_core::Action>, String> {
    host.send(text, move |piece| {
        // A closed channel only means the window went away; the turn still completes.
        let _ = on_token.send(piece.to_string());
    })
    .await
    .map_err(|e| e.to_string())
}

/// The bundled model if one is installed, otherwise the mock (the UI shows which).
#[cfg(target_os = "macos")]
fn backend(app: &tauri::AppHandle) -> Arc<dyn Backend> {
    for path in llama::model_candidates(app) {
        if !path.is_file() {
            continue;
        }
        match llama::LlamaBackend::load(&path) {
            Ok(backend) => {
                eprintln!("PR3: loaded {} from {}", backend.label(), path.display());
                return Arc::new(backend);
            }
            Err(e) => eprintln!("PR3: {e}; trying the next model location"),
        }
    }
    eprintln!(
        "PR3: no {} found; using the mock model (run scripts/fetch-model.sh)",
        llama::MODEL_FILE
    );
    Arc::new(MockBackend)
}

#[cfg(not(target_os = "macos"))]
fn backend(_app: &tauri::AppHandle) -> Arc<dyn Backend> {
    Arc::new(MockBackend)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .setup(|app| {
            let host = Host::new(backend(app.handle()));
            app.manage(host);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![platform_info, send_message])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("PR3 failed to start: {e}");
        std::process::exit(1);
    }
}
