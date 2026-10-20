//! PR3 reference app. The Rust side owns the runtime; the webview only renders.

mod host;
mod llama;

use std::sync::Arc;

use host::{Backend, Host, LoadingBackend, MockBackend};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{Emitter, Manager};

/// Event the UI listens to when the active model changes (e.g. loading finished).
const MODEL_CHANGED: &str = "model-changed";

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

/// Loads the bundled model in the background so the window appears at once; until it
/// is ready the UI shows "loading…", and if none is found it says where it looked.
fn start_model_loading(app: &tauri::AppHandle) {
    let candidates = llama::model_candidates(app);
    let Some(path) = candidates.iter().find(|p| p.is_file()).cloned() else {
        // Phones have no terminal, so the UI says where the model was expected.
        let expected = candidates
            .iter()
            .find(|p| !p.starts_with(env!("CARGO_MANIFEST_DIR")))
            .and_then(|p| p.parent())
            .map(|dir| dir.display().to_string());
        eprintln!(
            "PR3: no {} found; using the mock model (run scripts/fetch-model.sh)",
            llama::MODEL_FILE
        );
        set_backend(
            app,
            Arc::new(MockBackend::missing_model(llama::MODEL_FILE, expected)),
        );
        return;
    };

    let model = path.file_stem().map_or_else(
        || llama::MODEL_FILE.to_string(),
        |s| s.to_string_lossy().into_owned(),
    );
    set_backend(app, Arc::new(LoadingBackend { model }));

    let app = app.clone();
    std::thread::spawn(move || match llama::LlamaBackend::load(&path) {
        Ok(backend) => {
            eprintln!("PR3: loaded {} from {}", backend.label(), path.display());
            set_backend(&app, Arc::new(backend));
        }
        Err(e) => {
            eprintln!("PR3: {e}");
            set_backend(&app, Arc::new(MockBackend::failed_model(&e.to_string())));
        }
    });
}

fn set_backend(app: &tauri::AppHandle, backend: Arc<dyn Backend>) {
    let label = backend.label();
    app.state::<Host>().set_backend(backend);
    // The window may not be listening yet at startup; it also asks on load.
    let _ = app.emit(MODEL_CHANGED, label);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .setup(|app| {
            app.manage(Host::new(Arc::new(MockBackend::default())));
            start_model_loading(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![platform_info, send_message])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("PR3 failed to start: {e}");
        std::process::exit(1);
    }
}
