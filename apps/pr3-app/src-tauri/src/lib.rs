//! PR3 reference app. The Rust side owns the runtime; the webview only renders.

mod host;

use std::sync::Mutex;

use host::Host;
use serde::Serialize;

#[derive(Serialize)]
struct PlatformInfo {
    os: &'static str,
    arch: &'static str,
    version: &'static str,
}

#[tauri::command]
fn platform_info() -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        version: env!("CARGO_PKG_VERSION"),
    }
}

/// Runs one user turn through the core and returns what the UI should show.
#[tauri::command]
fn send_message(
    text: String,
    host: tauri::State<'_, Mutex<Host>>,
) -> Result<Vec<pr3_core::Action>, String> {
    let mut host = host
        .lock()
        .map_err(|_| "runtime state is unavailable".to_string())?;
    host.send(text).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .manage(Mutex::new(Host::new()))
        .invoke_handler(tauri::generate_handler![platform_info, send_message])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("PR3 failed to start: {e}");
        std::process::exit(1);
    }
}
