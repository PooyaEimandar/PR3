//! PR3 reference app. The Rust side owns the runtime; the webview only renders.

mod config;
mod host;
mod llama;
mod trace_export;

use std::sync::Arc;

use host::{Backend, Host, LoadingBackend, MockBackend, TurnEvent};
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
/// tokens and tool activity stream through `on_event` while the turn runs. The UI
/// passes the device's UTC offset so tools can tell local time without a tz database.
#[tauri::command]
async fn send_message(
    text: String,
    utc_offset_minutes: i32,
    on_event: Channel<TurnEvent>,
    host: tauri::State<'_, Host>,
) -> Result<Vec<pr3_core::Action>, String> {
    host.send(text, utc_offset_minutes, move |event| {
        // A closed channel only means the window went away; the turn still completes.
        let _ = on_event.send(event);
    })
    .await
    .map_err(|e| e.to_string())
}

/// Loads this device's model (config/pr3.toml) in the background so the window appears
/// at once; until it is ready the UI shows "loading…", and if none is found it says
/// where it looked.
fn start_model_loading(app: &tauri::AppHandle) {
    let data_dir = app.path().app_data_dir().ok();
    let device = config::Device::current();
    let chosen = config::Config::load(data_dir.as_deref()).and_then(|config| {
        config
            .model_for(device)
            .map(|(id, entry)| (id.to_string(), entry.file.clone()))
            .ok_or_else(|| "no model chosen for this device".to_string())
    });
    let (id, file) = match chosen {
        Ok(chosen) => chosen,
        Err(e) => {
            eprintln!("PR3: config: {e}");
            set_backend(app, Arc::new(MockBackend::failed_model(&e)));
            return;
        }
    };
    eprintln!("PR3: {device:?} runs {id} ({file})");

    let candidates = llama::model_candidates(data_dir.as_deref(), &file);
    let Some(path) = candidates.iter().find(|p| p.is_file()).cloned() else {
        // Phones have no terminal, so the UI says where the model was expected.
        let expected = candidates
            .iter()
            .find(|p| !p.starts_with(env!("CARGO_MANIFEST_DIR")))
            .and_then(|p| p.parent())
            .map(|dir| dir.display().to_string());
        eprintln!("PR3: no {file} found; using the mock model (run scripts/fetch-model.sh)");
        set_backend(app, Arc::new(MockBackend::missing_model(&file, expected)));
        return;
    };

    let model = path
        .file_stem()
        .map_or_else(|| file.clone(), |s| s.to_string_lossy().into_owned());
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

/// Finished turns are appended as OpenJarvis trace records to `traces.jsonl` in the app's
/// data folder, where OpenJarvis's analysis and learning tools can read them.
fn start_tracing(app: &tauri::AppHandle) {
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    let path = dir.join("traces.jsonl");
    app.state::<Host>().set_trace_sink(Arc::new(move |trace| {
        if let Err(e) = trace_export::append(&path, trace) {
            eprintln!("PR3: could not write trace to {}: {e}", path.display());
        }
    }));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            app.manage(Host::new(
                Arc::new(MockBackend::default()),
                pr3_tools::builtin(),
            )?);
            start_tracing(app.handle());
            start_model_loading(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![platform_info, send_message])
        .build(tauri::generate_context!());
    match app {
        Ok(app) => app.run(|handle, event| {
            // Free the model before the process exits: llama.cpp's Metal backend aborts
            // if its device is torn down while model buffers are still alive.
            if let tauri::RunEvent::Exit = event
                && let Some(host) = handle.try_state::<Host>()
            {
                host.set_backend(Arc::new(MockBackend::default()));
            }
        }),
        Err(e) => {
            eprintln!("PR3 failed to start: {e}");
            std::process::exit(1);
        }
    }
}
