//! PR3 reference app. The Rust side owns the runtime; the webview only renders.

mod actions;
#[cfg(target_os = "android")]
mod android_speech;
#[cfg(target_os = "android")]
mod android_vault;
mod backup;
mod config;
mod host;
mod hybrid;
mod llama;
mod memory;
mod models;
mod network;
mod share;
mod speech;
mod task;
mod trace_export;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use actions::Actions;
use host::{Backend, Host, MockBackend, TurnEvent};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{Emitter, Manager};

/// Event the UI listens to when the active model changes (e.g. loading finished).
const MODEL_CHANGED: &str = "model-changed";

#[derive(Serialize)]
struct PlatformInfo {
    os: &'static str,
    arch: &'static str,
    /// A CI build's version (script/version.py), such as
    /// 2026.10.27.30184-dev+fd9a2875dc06; none for a local build.
    version: Option<&'static str>,
    model: String,
}

#[tauri::command]
fn platform_info(host: tauri::State<'_, Host>) -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        version: option_env!("PR3_VERSION"),
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

/// On a computer: the node lending its model, if it runs, and where it listens.
struct NodeHandle(Option<Arc<pr3_hybrid::Node>>, String);

/// What the devices panel shows: on a computer, its node and paired phones; on a phone,
/// the computer it is paired with.
#[tauri::command]
fn devices_info(app: tauri::AppHandle) -> hybrid::DevicesInfo {
    if let Some(link) = app.try_state::<hybrid::Link>() {
        return hybrid::DevicesInfo::Phone {
            node: link.node().as_ref().map(Into::into),
        };
    }
    match app.try_state::<NodeHandle>() {
        Some(handle) => match &handle.0 {
            Some(node) => hybrid::DevicesInfo::Node {
                name: node.name().to_string(),
                listen: handle.1.clone(),
                phones: node.phones().iter().map(Into::into).collect(),
            },
            None => hybrid::DevicesInfo::Off,
        },
        None => hybrid::DevicesInfo::Off,
    }
}

#[derive(Serialize)]
struct PairingInfo {
    code: String,
    expires_in_s: u64,
    listen: String,
}

/// On a computer: a fresh pairing code to type on the phone.
#[tauri::command]
fn start_pairing(app: tauri::AppHandle) -> Result<PairingInfo, String> {
    let handle = app
        .try_state::<NodeHandle>()
        .ok_or("this device is not a node")?;
    let node = handle
        .0
        .as_ref()
        .ok_or("the node is off (see [node] in pr3.toml)")?;
    let code = node.start_pairing().map_err(|e| e.to_string())?;
    Ok(PairingInfo {
        code: code.code,
        expires_in_s: code.expires_in.as_secs(),
        listen: handle.1.clone(),
    })
}

/// On a computer: stops lending to a phone.
#[tauri::command]
fn forget_phone(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let handle = app
        .try_state::<NodeHandle>()
        .ok_or("this device is not a node")?;
    let node = handle.0.as_ref().ok_or("the node is off")?;
    node.forget(&id).map_err(|e| e.to_string())?;
    Ok(())
}

/// On a phone: pairs with the computer at `address` using the code it shows.
#[tauri::command]
async fn pair_with_node(
    app: tauri::AppHandle,
    address: String,
    code: String,
) -> Result<hybrid::PeerInfo, String> {
    let link = app
        .try_state::<hybrid::Link>()
        .ok_or("only a phone pairs with a computer")?
        .inner()
        .clone();
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let node = tauri::async_runtime::spawn_blocking(move || link.pair(&dir, &address, &code))
        .await
        .map_err(|e| e.to_string())??;
    let _ = app.emit(MODEL_CHANGED, ());
    Ok((&node).into())
}

/// On a phone: stops using the paired computer.
#[tauri::command]
fn unpair(app: tauri::AppHandle) -> Result<(), String> {
    let link = app
        .try_state::<hybrid::Link>()
        .ok_or("this device is not paired")?;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    link.unpair(&dir)?;
    let _ = app.emit(MODEL_CHANGED, ());
    Ok(())
}

/// A phone remembers the computer it paired with; a computer lends its model.
fn start_hybrid(app: &tauri::AppHandle, settings: &config::Node) {
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    if cfg!(mobile) {
        app.manage(hybrid::Link::open(&dir));
    } else {
        let node = hybrid::start_node(app, &dir, settings);
        app.manage(NodeHandle(node, settings.listen.clone()));
    }
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

/// Frees the model before the process exits: llama.cpp's Metal backend aborts if its
/// device is torn down while model buffers are alive. Stops any answer in progress, waits
/// for models still loading (which are then dropped, not installed) and for in-flight
/// work to let go of the model, then drops it. Bounded, so quitting never hangs.
fn shut_down(app: &tauri::AppHandle, host: &Host) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let model = host.shut_down();
    let loaders = app
        .try_state::<models::Models>()
        .map(|m| m.take_loaders())
        .unwrap_or_default();
    for loader in loaders {
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
    // A paired phone asks its computer first and falls back to this model.
    let backend: Arc<dyn Backend> = match app.try_state::<hybrid::Link>() {
        Some(link) => Arc::new(hybrid::Delegating::new(link.inner().clone(), backend)),
        None => backend,
    };
    let label = backend.label();
    app.state::<Host>().set_backend(backend);
    // The window may not be listening yet at startup; it also asks on load.
    let _ = app.emit(MODEL_CHANGED, label);
}

/// The memory store in the app's data folder. Without it PR3 still runs; it just
/// cannot remember.
fn open_memory(app: &tauri::AppHandle) -> Option<Arc<Mutex<pr3_memory_sqlite::SqliteMemory>>> {
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

/// The app's host, on the memory store and trusted path in its data folder.
fn host(
    app: &tauri::AppHandle,
    actions: Option<&config::Actions>,
    tasks: config::Tasks,
) -> Result<Host, host::HostError> {
    let store = open_memory(app);
    if let Some(store) = &store {
        let memory: pr3_tool::SharedMemory = store.clone();
        // The panel where the user reviews what PR3 remembers uses the same store.
        app.manage(memory::MemoryHandle(memory));
    }
    let actions = actions.and_then(|settings| open_actions(app, settings));
    compose_host(
        Arc::new(MockBackend::default()),
        store,
        actions,
        tasks.keep_days,
    )
}

/// The host with the built-in tools, plus memory tools, per-message recall and kept
/// tasks when there is a memory store, and consequential actions when there is a
/// trusted path. The task benchmark builds the same host, so it measures this one.
pub fn compose_host(
    backend: Arc<dyn Backend>,
    store: Option<Arc<Mutex<pr3_memory_sqlite::SqliteMemory>>>,
    actions: Option<Arc<Actions>>,
    keep_days: u64,
) -> Result<Host, host::HostError> {
    let host = match store {
        Some(store) => {
            let memory: pr3_tool::SharedMemory = store.clone();
            let mut tools = pr3_tool::builtin();
            tools.extend(pr3_tool::memory_tools(Arc::clone(&memory)));
            Host::new(backend, tools)?
                .with_memory(memory)
                .with_tasks(store, keep_days)
        }
        None => Host::new(backend, pr3_tool::builtin())?,
    };
    match actions {
        Some(actions) => host.with_actions(actions),
        None => Ok(host),
    }
}

/// What the task benchmark (bench/task) drives besides [`compose_host`]. Benchmark
/// only: the app itself reaches these through their own modules.
#[doc(hidden)]
pub mod bench {
    pub use crate::actions::Actions;
    pub use crate::host::{Backend, Generation, Host, HostError, SUMMARY, TurnEvent};
    pub use crate::llama::LlamaBackend;
    /// The runtime the host's async calls run on.
    pub use tauri::async_runtime::block_on;
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
    // Earlier builds kept models in `models/`; the folder is `model/` now.
    let (old, new) = (dir.join("models"), dir.join("model"));
    if old.is_dir()
        && !new.exists()
        && let Err(e) = std::fs::rename(&old, &new)
    {
        eprintln!(
            "PR3: could not move {} to {}: {e}",
            old.display(),
            new.display()
        );
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "android")]
    let builder = builder
        .plugin(android_vault::plugin())
        .plugin(android_speech::plugin())
        .plugin(network::plugin())
        .plugin(share::plugin());
    let app = builder
        .setup(|app| {
            prepare_data_dir(app.handle());
            // An export waits in the cache for the share sheet; it holds a chat in plain text.
            if let Ok(dir) = task::export::dir(app.handle()) {
                task::export::clear(&dir);
            }
            let data_dir = app.path().app_data_dir().ok();
            let config = config::Config::load(data_dir.as_deref());
            let actions = config.as_ref().ok().map(|c| &c.actions);
            let tasks = config.as_ref().map(|c| c.tasks).unwrap_or_default();
            app.manage(host(app.handle(), actions, tasks)?);
            let node = config.as_ref().map(|c| c.node.clone()).unwrap_or_default();
            start_hybrid(app.handle(), &node);
            start_tracing(
                app.handle(),
                config.as_ref().map(|c| c.traces).unwrap_or_default(),
            );
            app.manage(models::Models::new(data_dir.as_deref(), config));
            app.state::<models::Models>().start(app.handle());
            app.manage(speech::Speech::open(app.handle()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            platform_info,
            send_message,
            answer_approval,
            devices_info,
            start_pairing,
            forget_phone,
            pair_with_node,
            unpair,
            speech::speech_status,
            speech::speech_languages,
            speech::set_speech_language,
            speech::start_listening,
            speech::stop_listening,
            speech::cancel_listening,
            speech::speak,
            speech::stop_speaking,
            memory::list_memories,
            memory::add_memory,
            memory::confirm_memory,
            memory::correct_memory,
            memory::forget_memory,
            models::list_models,
            models::download_model,
            models::cancel_download,
            models::set_mobile_data,
            models::use_model,
            models::delete_model,
            task::list_tasks,
            task::open_task,
            task::current_task,
            task::ask_again,
            task::start_new_task,
            task::new_topic,
            task::continue_task,
            task::follow_up_task,
            task::task_words,
            task::export_task,
            task::delete_task
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
