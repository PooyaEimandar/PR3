//! PR3 reference app. The Rust side owns the runtime; the webview only renders.

mod actions;
#[cfg(target_os = "android")]
mod android_vault;
mod backup;
mod config;
mod host;
mod llama;
mod trace_export;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use actions::Actions;
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

/// The user's answer on the confirmation card for a consequential action.
#[tauri::command]
fn answer_approval(
    request: String,
    approve: bool,
    host: tauri::State<'_, Host>,
) -> Result<(), String> {
    host.answer_approval(&request, approve)
}

/// Loads this device's model (config/pr3.toml) in the background so the window appears
/// at once; until it is ready the UI shows "loading…", and if none is found it says
/// where it looked.
fn start_model_loading(app: &tauri::AppHandle, config: Result<config::Config, String>) {
    let data_dir = app.path().app_data_dir().ok();
    let device = config::Device::current();
    let chosen = config.and_then(|config| {
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
        let expected = data_dir
            .as_ref()
            .map(|dir| dir.join("models").display().to_string());
        eprintln!("PR3: no {file} found; using the mock model (run scripts/fetch-model.sh)");
        set_backend(app, Arc::new(MockBackend::missing_model(&file, expected)));
        return;
    };

    let model = path
        .file_stem()
        .map_or_else(|| file.clone(), |s| s.to_string_lossy().into_owned());
    set_backend(app, Arc::new(LoadingBackend { model }));

    let handle = app.clone();
    let loader = std::thread::spawn(move || match llama::LlamaBackend::load(&path) {
        Ok(backend) => {
            eprintln!("PR3: loaded {} from {}", backend.label(), path.display());
            set_backend(&handle, Arc::new(backend));
        }
        Err(e) => {
            eprintln!("PR3: {e}");
            set_backend(&handle, Arc::new(MockBackend::failed_model(&e.to_string())));
        }
    });
    app.manage(ModelLoader(Mutex::new(Some(loader))));
}

/// The thread loading the model, so quitting can wait for it.
struct ModelLoader(Mutex<Option<JoinHandle<()>>>);

/// Frees the model before the process exits: llama.cpp's Metal backend aborts if its
/// device is torn down while model buffers are alive. Stops any answer in progress, waits
/// for a model still loading (which is then dropped, not installed) and for in-flight
/// work to let go of the model, then drops it. Bounded, so quitting never hangs.
fn shut_down(app: &tauri::AppHandle, host: &Host) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let model = host.shut_down();
    let loader = app
        .try_state::<ModelLoader>()
        .and_then(|l| l.0.lock().ok().and_then(|mut h| h.take()));
    if let Some(loader) = loader {
        while !loader.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if loader.is_finished() {
            let _ = loader.join();
        }
    }
    while Arc::strong_count(&model) > 1 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(model);
}

fn set_backend(app: &tauri::AppHandle, backend: Arc<dyn Backend>) {
    let label = backend.label();
    app.state::<Host>().set_backend(backend);
    // The window may not be listening yet at startup; it also asks on load.
    let _ = app.emit(MODEL_CHANGED, label);
}

/// The memory store in the app's data folder. Without it PR3 still runs; it just
/// cannot remember.
fn open_memory(app: &tauri::AppHandle) -> Option<pr3_tools::SharedMemory> {
    let opened = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())
        .and_then(|dir| {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let path = dir.join("memory.sqlite");
            pr3_memory_sqlite::SqliteMemory::open(&path)
                .map(|store| (store, path))
                .map_err(|e| e.to_string())
        });
    match opened {
        Ok((store, path)) => {
            eprintln!("PR3: memory at {}", path.display());
            Some(Arc::new(Mutex::new(store)))
        }
        Err(e) => {
            eprintln!("PR3: memory unavailable: {e}");
            None
        }
    }
}

/// This device's approval key: in the Secure Enclave on Apple devices, in the Keystore
/// on Android, signing only after Face ID, Touch ID, a fingerprint or the passcode.
fn platform_vault(app: &tauri::AppHandle, dir: &Path) -> Result<Box<dyn pr3_vault::Vault>, String> {
    #[cfg(target_os = "android")]
    {
        let plugin = app
            .try_state::<android_vault::VaultHandle>()
            .ok_or("the vault plugin is not loaded")?;
        let vault = android_vault::KeystoreVault::open(plugin.0.clone())
            .map_err(|e| format!("{e} (set a screen lock to approve actions)"))?;
        // A software key from earlier builds is no longer needed.
        match std::fs::remove_file(dir.join(pr3_vault::SOFTWARE_KEY_FILE)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
            _ => {}
        }
        Ok(Box::new(vault))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        pr3_vault::open_platform(dir).map_err(|e| e.to_string())
    }
}

/// The trusted path for consequential actions, with its vault, outbox and audit log
/// in the app's data folder. Without it PR3 runs, but cannot act.
fn open_actions(app: &tauri::AppHandle, settings: &config::Actions) -> Option<Arc<Actions>> {
    let opened = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())
        .and_then(|dir| {
            let vault = platform_vault(app, &dir)?;
            Actions::with_vault(&dir, settings.limits(), settings.directory(), vault)
        });
    match opened {
        Ok(actions) => Some(Arc::new(actions)),
        Err(e) => {
            eprintln!("PR3: actions unavailable: {e}");
            None
        }
    }
}

/// The host with the built-in tools, plus memory tools and per-message recall when
/// memory is available, and consequential actions when they are configured.
fn host(
    app: &tauri::AppHandle,
    actions: Option<&config::Actions>,
) -> Result<Host, host::HostError> {
    let backend = Arc::new(MockBackend::default());
    let host = match open_memory(app) {
        Some(memory) => {
            let mut tools = pr3_tools::builtin();
            tools.extend(pr3_tools::memory_tools(Arc::clone(&memory)));
            Host::new(backend, tools)?.with_memory(memory)
        }
        None => Host::new(backend, pr3_tools::builtin())?,
    };
    match actions.and_then(|settings| open_actions(app, settings)) {
        Some(actions) => host.with_actions(actions),
        None => Ok(host),
    }
}

/// Finished turns are appended as OpenJarvis trace records to `traces.jsonl` in the app's
/// data folder, where OpenJarvis's analysis and learning tools can read them.
fn start_tracing(app: &tauri::AppHandle, settings: config::Traces) {
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    if !settings.enabled {
        return;
    }
    let path = dir.join("traces.jsonl");
    let max_bytes = settings.max_mb.saturating_mul(1024 * 1024);
    eprintln!("PR3: tracing to {}", path.display());
    app.state::<Host>().set_trace_sink(Arc::new(move |trace| {
        if let Err(e) = trace_export::append(&path, trace, max_bytes) {
            eprintln!("PR3: could not write trace to {}: {e}", path.display());
        }
    }));
}

/// Creates the app's data folder and keeps it out of backups (see [`backup`]).
fn prepare_data_dir(app: &tauri::AppHandle) {
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    let ready = std::fs::create_dir_all(&dir)
        .map_err(|e| e.to_string())
        .and_then(|()| backup::exclude(&dir));
    if let Err(e) = ready {
        eprintln!("PR3: data folder {}: {e}", dir.display());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "android")]
    let builder = builder.plugin(android_vault::plugin());
    let app = builder
        .setup(|app| {
            prepare_data_dir(app.handle());
            let data_dir = app.path().app_data_dir().ok();
            let config = config::Config::load(data_dir.as_deref());
            let actions = config.as_ref().ok().map(|c| &c.actions);
            app.manage(host(app.handle(), actions)?);
            start_tracing(
                app.handle(),
                config.as_ref().map(|c| c.traces).unwrap_or_default(),
            );
            start_model_loading(app.handle(), config);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            platform_info,
            send_message,
            answer_approval
        ])
        .build(tauri::generate_context!());
    match app {
        Ok(app) => app.run(|handle, event| {
            if let tauri::RunEvent::Exit = event
                && let Some(host) = handle.try_state::<Host>()
            {
                shut_down(handle, &host);
            }
        }),
        Err(e) => {
            eprintln!("PR3 failed to start: {e}");
            std::process::exit(1);
        }
    }
}
