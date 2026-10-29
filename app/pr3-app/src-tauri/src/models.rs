//! The models on this device: which of config/pr3.toml's models are downloaded, which
//! one runs, and downloading, choosing and deleting them in Settings. They live in the
//! `model/` folder of the app's data folder, apart from the app, so installing an update
//! leaves them in place and deleting one never touches the app. (Uninstalling removes the
//! data folder, and the models with it.)
//!
//! A model is downloaded only from its pinned repository and revision, over HTTPS, and
//! gets its file name only once its size and SHA-256 match the config, so nothing else
//! is ever loaded as a model. An interrupted download resumes where it stopped. On a
//! phone, a download waits for Wi-Fi unless the user allows mobile data in Settings.

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{Emitter, Manager};

use crate::config::{Config, Device, ModelEntry};
use crate::host::{Backend, LoadingBackend, MockBackend};
use crate::llama;
use crate::network;

/// Event the UI listens to while a model downloads.
const MODEL_DOWNLOAD: &str = "model-download";
/// How often a download reports its progress.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);
/// The file in the model folder that holds the id of the model the user chose.
const SELECTED: &str = "selected";
/// The file in the model folder that lets downloads use mobile data, if it is there.
const MOBILE_DATA: &str = "mobile-data";
/// How often a download waiting for Wi-Fi looks again.
const WAIT_STEP: Duration = Duration::from_millis(500);
/// How often a running download checks that the phone is still off mobile data.
const CHECK_EVERY: Duration = if cfg!(test) {
    Duration::ZERO
} else {
    Duration::from_secs(2)
};
/// The share of memory a model file may take: a phone's app gets a little over half of
/// its memory before the system ends it, and the model needs room to work in.
const MOBILE_SHARE: f64 = 0.5;
const DESKTOP_SHARE: f64 = 0.75;

/// Why a download stopped.
#[derive(Debug)]
pub enum DownloadError {
    Cancelled,
    /// The server answered with something else than the file.
    Http(String),
    Io(io::Error),
    /// The file arrived whole but is not the pinned one, and was removed.
    Mismatch,
}

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "cancelled"),
            Self::Http(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Mismatch => write!(
                f,
                "the download did not match the model's SHA-256 and was removed"
            ),
        }
    }
}

impl From<io::Error> for DownloadError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Where a model's download accumulates. Named by checksum, like script/fetch-model.sh's,
/// so a resumed download never appends to a different file.
pub fn partial_path(dest: &Path, sha256: &str) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{sha256}.partial"));
    dest.with_file_name(name)
}

/// The client for model downloads: HTTPS only, and a stalled connection fails after a
/// minute rather than hanging, leaving the download to resume.
pub fn agent() -> ureq::Agent {
    agent_for(true)
}

fn agent_for(https_only: bool) -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(https_only)
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        .timeout_per_read(Some(Duration::from_secs(60)))
        .user_agent(concat!("PR3/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Downloads `url` to `dest`, resuming a partial download, and gives it its name only if
/// it is `size` bytes with SHA-256 `sha256`. `progress` gets the bytes held so far.
/// Stopping, when `stop` says so, or losing the connection keeps what arrived, for the
/// next try.
pub fn download(
    agent: &ureq::Agent,
    url: &str,
    dest: &Path,
    size: u64,
    sha256: &str,
    stop: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(u64),
) -> Result<(), DownloadError> {
    let partial = partial_path(dest, sha256);
    let mut have = fs::metadata(&partial).map_or(0, |m| m.len());
    if have > size {
        fs::remove_file(&partial)?;
        have = 0;
    }
    let mut hasher = Sha256::new();
    if have > 0 {
        hash_file(&partial, have, &mut hasher, stop)?;
    }
    progress(have);

    let mut request = agent.get(url);
    if have > 0 {
        request = request.header("Range", format!("bytes={have}-"));
    }
    let response = request
        .call()
        .map_err(|e| DownloadError::Http(e.to_string()))?;
    let status = response.status().as_u16();
    match status {
        // The rest of the file, from where the partial one ends.
        206 if have > 0 => {
            let range = response
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default();
            if !range.starts_with(&format!("bytes {have}-")) {
                return Err(DownloadError::Http(format!(
                    "the server resumed at the wrong place ({range})"
                )));
            }
        }
        // The whole file: the server does not resume, so start over.
        200 => {
            have = 0;
            hasher = Sha256::new();
        }
        // Nothing left to send: the partial file is whole.
        416 if have == size => {}
        _ => return Err(DownloadError::Http(format!("the server answered {status}"))),
    }

    if status != 416 {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&partial)?;
        file.set_len(have)?;
        file.seek(SeekFrom::Start(have))?;
        let mut body = response.into_body().into_reader();
        let mut buf = vec![0; 1 << 20];
        loop {
            if stop() {
                return Err(DownloadError::Cancelled);
            }
            let n = body.read(&mut buf)?;
            if n == 0 {
                break;
            }
            if have + n as u64 > size {
                drop(file);
                fs::remove_file(&partial)?;
                return Err(DownloadError::Mismatch);
            }
            file.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            have += n as u64;
            progress(have);
        }
        file.sync_all()?;
        if have < size {
            return Err(DownloadError::Http(
                "the connection closed early; the download resumes from here".to_string(),
            ));
        }
    }

    let digest = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if !digest.eq_ignore_ascii_case(sha256) {
        fs::remove_file(&partial)?;
        return Err(DownloadError::Mismatch);
    }
    fs::rename(&partial, dest)?;
    Ok(())
}

/// Feeds the first `len` bytes of `path` to `hasher`, stopping early if told to.
fn hash_file(
    path: &Path,
    len: u64,
    hasher: &mut Sha256,
    stop: &dyn Fn() -> bool,
) -> Result<(), DownloadError> {
    let mut file = File::open(path)?.take(len);
    let mut buf = vec![0; 1 << 20];
    loop {
        if stop() {
            return Err(DownloadError::Cancelled);
        }
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        hasher.update(&buf[..n]);
    }
}

/// Downloads like `download`, but only while `costly` says the network costs nothing:
/// until then it waits, and a download under way when the network turns costly stops
/// and waits again, keeping what arrived. `waiting` hears when it starts and stops
/// waiting. Only `cancel` ends it early.
#[allow(clippy::too_many_arguments)]
pub fn download_when_free(
    agent: &ureq::Agent,
    url: &str,
    dest: &Path,
    size: u64,
    sha256: &str,
    cancel: &AtomicBool,
    costly: &dyn Fn() -> bool,
    waiting: &mut dyn FnMut(bool),
    progress: &mut dyn FnMut(u64),
) -> Result<(), DownloadError> {
    loop {
        if costly() {
            waiting(true);
            while costly() {
                if cancel.load(Ordering::Relaxed) {
                    return Err(DownloadError::Cancelled);
                }
                std::thread::sleep(WAIT_STEP);
            }
            waiting(false);
        }
        let paused = Cell::new(false);
        let checked = Cell::new(Instant::now());
        let stop = || {
            if cancel.load(Ordering::Relaxed) {
                return true;
            }
            if !paused.get() && checked.get().elapsed() >= CHECK_EVERY {
                checked.set(Instant::now());
                paused.set(costly());
            }
            paused.get()
        };
        match download(agent, url, dest, size, sha256, &stop, progress) {
            // Not cancelled: the network turned costly, so wait for a free one.
            Err(DownloadError::Cancelled) if !cancel.load(Ordering::Relaxed) => {}
            result => return result,
        }
    }
}

/// Whether a model file of `size` bytes leaves this device room to run it.
pub fn fits(device: Device, size: u64) -> bool {
    let (memory, share) = match device {
        Device::Desktop { memory } => (memory, DESKTOP_SHARE),
        Device::Mobile { memory } => (memory, MOBILE_SHARE),
    };
    memory.is_none_or(|m| size as f64 <= m as f64 * share)
}

/// A download in progress, or one that failed.
struct Download {
    bytes: u64,
    running: bool,
    /// Waiting for Wi-Fi.
    waiting: bool,
    error: Option<String>,
    cancel: Arc<AtomicBool>,
}

/// What the device runs: the model it chose and loaded, or none.
#[derive(Default)]
struct Running {
    /// The config id of the model running or loading, if it came from the config.
    id: Option<String>,
    /// A model is running or loading (from the config or `PR3_MODEL`), not the stand-in.
    real: bool,
}

/// The models on this device, kept by the app for the Settings panel.
pub struct Models {
    /// The `model/` folder in the app's data folder.
    dir: Option<PathBuf>,
    config: Result<Config, String>,
    device: Device,
    /// A `.gguf` file named by `PR3_MODEL`, which overrides any choice.
    forced: Option<PathBuf>,
    /// The user lets downloads use mobile data (phones only).
    mobile_data: AtomicBool,
    downloads: Mutex<HashMap<String, Download>>,
    /// Held while a model is chosen or installed, so they happen in order.
    running: Mutex<Running>,
    /// Bumped by each load, so a slower earlier load never replaces a later choice.
    generation: AtomicU64,
    loaders: Mutex<Vec<JoinHandle<()>>>,
}

/// One model as the Models panel shows it.
#[derive(Serialize)]
pub struct ModelView {
    id: String,
    file: String,
    size: u64,
    /// The one config/pr3.toml picks for this device.
    recommended: bool,
    /// Small enough for this device's memory.
    fits: bool,
    /// In the app's model folder.
    downloaded: bool,
    /// Found only in the repository's model folder (debug builds): usable, not deletable.
    in_repository: bool,
    /// Bytes downloaded so far, if a download is unfinished.
    partial: u64,
    downloading: bool,
    /// Downloading, but waiting for Wi-Fi.
    waiting: bool,
    /// Why the last download stopped.
    error: Option<String>,
    /// Running, or loading to run.
    active: bool,
}

/// What the Models panel shows.
#[derive(Serialize)]
pub struct ModelsInfo {
    /// Where the models are kept.
    folder: Option<String>,
    /// `PR3_MODEL` chooses the model, so the panel cannot.
    forced: Option<String>,
    /// Why the config could not be read, if it could not.
    problem: Option<String>,
    /// Whether downloads may use mobile data; none on a computer, which never asks.
    mobile_data: Option<bool>,
    models: Vec<ModelView>,
}

/// Progress of a download, for the UI.
#[derive(Clone, Serialize)]
struct Progress {
    id: String,
    bytes: u64,
    size: u64,
    done: bool,
    /// Waiting for Wi-Fi.
    waiting: bool,
    error: Option<String>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Models {
    pub fn new(data_dir: Option<&Path>, config: Result<Config, String>) -> Self {
        Self {
            dir: data_dir.map(|d| d.join("model")),
            config,
            device: Device::current(),
            forced: std::env::var_os("PR3_MODEL").map(PathBuf::from),
            mobile_data: AtomicBool::new(
                data_dir.is_some_and(|d| d.join("model").join(MOBILE_DATA).is_file()),
            ),
            downloads: Mutex::default(),
            running: Mutex::default(),
            generation: AtomicU64::new(0),
            loaders: Mutex::default(),
        }
    }

    fn config(&self) -> Result<&Config, String> {
        self.config.as_ref().map_err(Clone::clone)
    }

    fn entry(&self, id: &str) -> Result<&ModelEntry, String> {
        self.config()?
            .models
            .get(id)
            .ok_or_else(|| format!("no model '{id}' in the config"))
    }

    fn dir(&self) -> Result<&Path, String> {
        self.dir
            .as_deref()
            .ok_or_else(|| "this device has no data folder".to_string())
    }

    /// The downloaded file, in the app's model folder.
    fn downloaded(&self, entry: &ModelEntry) -> Option<PathBuf> {
        let path = self.dir.as_ref()?.join(&entry.file);
        path.is_file().then_some(path)
    }

    /// The file to load: downloaded, or in debug builds the repository's copy.
    fn locate(&self, entry: &ModelEntry) -> Option<PathBuf> {
        self.downloaded(entry).or_else(|| {
            #[cfg(debug_assertions)]
            {
                let path = llama::repo_models().join(&entry.file);
                if path.is_file() {
                    return Some(path);
                }
            }
            None
        })
    }

    fn selected(&self) -> Option<String> {
        let text = fs::read_to_string(self.dir.as_ref()?.join(SELECTED)).ok()?;
        Some(text.trim().to_string()).filter(|id| !id.is_empty())
    }

    fn select(&self, id: Option<&str>) -> Result<(), String> {
        let path = self.dir()?.join(SELECTED);
        let result = match id {
            Some(id) => fs::create_dir_all(self.dir()?).and_then(|()| fs::write(&path, id)),
            None => match fs::remove_file(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            },
        };
        result.map_err(|e| e.to_string())
    }

    /// Loads the model this device should run: `PR3_MODEL`, else the one the user chose,
    /// else the one the config picks for this device; without one, the stand-in says
    /// what to download.
    pub fn start(&self, app: &tauri::AppHandle) {
        if let Some(path) = self.forced.clone() {
            eprintln!("PR3: PR3_MODEL chooses {}", path.display());
            self.load(app, None, path);
            return;
        }
        let config = match self.config() {
            Ok(config) => config,
            Err(e) => {
                eprintln!("PR3: config: {e}");
                self.install_stand_in(app, MockBackend::failed_model(&e));
                return;
            }
        };
        let chosen = self.selected().and_then(|id| {
            let entry = config.models.get(&id)?;
            let path = self.locate(entry)?;
            fits(self.device, entry.size).then_some((id, path))
        });
        if let Some((id, path)) = chosen {
            eprintln!("PR3: runs {id}, as chosen in Settings");
            self.load(app, Some(id), path);
            return;
        }
        let Some((id, entry)) = config.model_for(self.device) else {
            self.install_stand_in(app, MockBackend::failed_model("no model for this device"));
            return;
        };
        eprintln!("PR3: {:?} runs {id} ({})", self.device, entry.file);
        match self.locate(entry) {
            Some(path) => self.load(app, Some(id.to_string()), path),
            None => {
                eprintln!("PR3: no {} found; using the stand-in", entry.file);
                self.install_stand_in(app, MockBackend::missing_model(&entry.file));
            }
        }
    }

    fn install_stand_in(&self, app: &tauri::AppHandle, mock: MockBackend) {
        let mut running = lock(&self.running);
        self.generation.fetch_add(1, Ordering::SeqCst);
        *running = Running::default();
        crate::set_backend(app, Arc::new(mock));
    }

    /// Loads `path` in the background; until it is ready the UI shows "loading…". The
    /// previous model is let go first, so two never fill the memory together.
    fn load(&self, app: &tauri::AppHandle, id: Option<String>, path: PathBuf) {
        let ticket = {
            let mut running = lock(&self.running);
            let ticket = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            *running = Running { id, real: true };
            let model = path
                .file_stem()
                .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
            crate::set_backend(app, Arc::new(LoadingBackend { model }));
            ticket
        };
        let handle = app.clone();
        let loader = std::thread::spawn(move || {
            let loaded = llama::LlamaBackend::load(&path);
            let models = handle.state::<Models>();
            let mut running = lock(&models.running);
            if models.generation.load(Ordering::SeqCst) != ticket {
                // Another choice came meanwhile; this model is dropped unused.
                return;
            }
            match loaded {
                Ok(backend) => {
                    eprintln!("PR3: loaded {} from {}", backend.label(), path.display());
                    crate::set_backend(&handle, Arc::new(backend));
                }
                Err(e) => {
                    eprintln!("PR3: {e}");
                    *running = Running::default();
                    crate::set_backend(
                        &handle,
                        Arc::new(MockBackend::failed_model(&e.to_string())),
                    );
                }
            }
        });
        let mut loaders = lock(&self.loaders);
        loaders.retain(|l| !l.is_finished());
        loaders.push(loader);
    }

    /// The threads still loading a model, so quitting can wait for them.
    pub fn take_loaders(&self) -> Vec<JoinHandle<()>> {
        std::mem::take(&mut *lock(&self.loaders))
    }

    pub fn info(&self) -> ModelsInfo {
        let config = match self.config() {
            Ok(config) => config,
            Err(e) => {
                return ModelsInfo {
                    folder: None,
                    forced: None,
                    problem: Some(e),
                    mobile_data: self.mobile_data(),
                    models: Vec::new(),
                };
            }
        };
        let recommended = config.model_for(self.device).map(|(id, _)| id);
        let running = lock(&self.running);
        let downloads = lock(&self.downloads);
        let models = config
            .models
            .iter()
            .map(|(id, entry)| {
                let download = downloads.get(id);
                let downloaded = self.downloaded(entry).is_some();
                let partial = self.dir.as_ref().map_or(0, |dir| {
                    fs::metadata(partial_path(&dir.join(&entry.file), &entry.sha256))
                        .map_or(0, |m| m.len())
                });
                ModelView {
                    id: id.clone(),
                    file: entry.file.clone(),
                    size: entry.size,
                    recommended: recommended == Some(id.as_str()),
                    fits: fits(self.device, entry.size),
                    downloaded,
                    in_repository: !downloaded && self.locate(entry).is_some(),
                    partial: download.filter(|d| d.running).map_or(partial, |d| d.bytes),
                    downloading: download.is_some_and(|d| d.running),
                    waiting: download.is_some_and(|d| d.running && d.waiting),
                    error: download.and_then(|d| d.error.clone()),
                    active: running.id.as_deref() == Some(id.as_str()),
                }
            })
            .collect();
        ModelsInfo {
            folder: self.dir.as_ref().map(|d| d.display().to_string()),
            forced: self.forced.as_ref().map(|p| p.display().to_string()),
            problem: None,
            mobile_data: self.mobile_data(),
            models,
        }
    }

    fn mobile_data(&self) -> Option<bool> {
        network::PHONE.then(|| self.mobile_data.load(Ordering::Relaxed))
    }

    /// Lets downloads use mobile data, or makes them wait for Wi-Fi, from now on and on
    /// later starts. A download waiting for Wi-Fi goes on at once when allowed.
    pub fn set_mobile_data(&self, allowed: bool) -> Result<(), String> {
        let dir = self.dir()?;
        let path = dir.join(MOBILE_DATA);
        let result = if allowed {
            fs::create_dir_all(dir).and_then(|()| fs::write(&path, "allowed\n"))
        } else {
            match fs::remove_file(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        };
        result.map_err(|e| e.to_string())?;
        self.mobile_data.store(allowed, Ordering::Relaxed);
        Ok(())
    }

    /// Starts downloading model `id` in the background; on a phone on mobile data it waits
    /// for Wi-Fi unless the user allows mobile data. When it is whole and verified, it
    /// runs at once if the device had no model yet.
    pub fn download(&self, app: &tauri::AppHandle, id: &str) -> Result<(), String> {
        let entry = self.entry(id)?.clone();
        if !fits(self.device, entry.size) {
            return Err(format!(
                "{} is too large for this device's memory",
                entry.file
            ));
        }
        let dir = self.dir()?.to_path_buf();
        let dest = dir.join(&entry.file);
        if dest.is_file() {
            return Err(format!("{} is already downloaded", entry.file));
        }
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut downloads = lock(&self.downloads);
            if downloads.get(id).is_some_and(|d| d.running) {
                return Err(format!("{} is already downloading", entry.file));
            }
            downloads.insert(
                id.to_string(),
                Download {
                    bytes: fs::metadata(partial_path(&dest, &entry.sha256)).map_or(0, |m| m.len()),
                    running: true,
                    waiting: false,
                    error: None,
                    cancel: Arc::clone(&cancel),
                },
            );
        }
        let handle = app.clone();
        let id = id.to_string();
        std::thread::spawn(move || {
            let models = handle.state::<Models>();
            let mut last = Instant::now() - PROGRESS_EVERY;
            let costly = || {
                network::PHONE
                    && !models.mobile_data.load(Ordering::Relaxed)
                    && network::metered(&handle)
            };
            let result = download_when_free(
                &agent(),
                &entry.url(),
                &dest,
                entry.size,
                &entry.sha256,
                &cancel,
                &costly,
                &mut |waiting| {
                    let bytes = {
                        let mut downloads = lock(&models.downloads);
                        let download = downloads.get_mut(&id);
                        download.map_or(0, |d| {
                            d.waiting = waiting;
                            d.bytes
                        })
                    };
                    let _ = handle.emit(
                        MODEL_DOWNLOAD,
                        Progress {
                            id: id.clone(),
                            bytes,
                            size: entry.size,
                            done: false,
                            waiting,
                            error: None,
                        },
                    );
                },
                &mut |bytes| {
                    if let Some(d) = lock(&models.downloads).get_mut(&id) {
                        d.bytes = bytes;
                    }
                    if last.elapsed() >= PROGRESS_EVERY {
                        last = Instant::now();
                        let _ = handle.emit(
                            MODEL_DOWNLOAD,
                            Progress {
                                id: id.clone(),
                                bytes,
                                size: entry.size,
                                done: false,
                                waiting: false,
                                error: None,
                            },
                        );
                    }
                },
            );
            let error = match &result {
                Ok(()) | Err(DownloadError::Cancelled) => None,
                Err(e) => Some(e.to_string()),
            };
            {
                let mut downloads = lock(&models.downloads);
                match &error {
                    Some(e) => {
                        eprintln!("PR3: downloading {}: {e}", entry.file);
                        if let Some(d) = downloads.get_mut(&id) {
                            d.running = false;
                            d.error = Some(e.clone());
                        }
                    }
                    None => {
                        downloads.remove(&id);
                    }
                }
            }
            if result.is_ok() {
                eprintln!("PR3: downloaded and verified {}", dest.display());
                // The first model on a device runs at once.
                let idle = !lock(&models.running).real;
                if idle {
                    let _ = models.select(Some(&id));
                    models.load(&handle, Some(id.clone()), dest);
                }
            }
            let _ = handle.emit(
                MODEL_DOWNLOAD,
                Progress {
                    id,
                    bytes: 0,
                    size: entry.size,
                    done: true,
                    waiting: false,
                    error,
                },
            );
        });
        Ok(())
    }

    /// Stops a download; what arrived stays, so it resumes later.
    pub fn cancel(&self, id: &str) {
        if let Some(d) = lock(&self.downloads).get(id) {
            d.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Runs model `id` from now on, and on later starts.
    pub fn choose(&self, app: &tauri::AppHandle, id: &str) -> Result<(), String> {
        if self.forced.is_some() {
            return Err("PR3_MODEL chooses the model; unset it to choose here".to_string());
        }
        let entry = self.entry(id)?;
        if !fits(self.device, entry.size) {
            return Err(format!(
                "{} is too large for this device's memory",
                entry.file
            ));
        }
        let path = self
            .locate(entry)
            .ok_or_else(|| format!("{} is not downloaded", entry.file))?;
        self.select(Some(id))?;
        self.load(app, Some(id.to_string()), path);
        Ok(())
    }

    /// Deletes model `id` from this device, with any unfinished download of it. If it
    /// was running, the device goes back to the model it would start with.
    pub fn delete(&self, app: &tauri::AppHandle, id: &str) -> Result<(), String> {
        let entry = self.entry(id)?.clone();
        let dest = self.dir()?.join(&entry.file);
        self.cancel(id);
        lock(&self.downloads).remove(id);
        let was_running = lock(&self.running).id.as_deref() == Some(id);
        if was_running {
            // Let go of the model first: Windows cannot delete a file that is mapped.
            self.install_stand_in(app, MockBackend::failed_model("its model was deleted"));
        }
        if self.selected().as_deref() == Some(id) {
            self.select(None)?;
        }
        let mut problem = None;
        for path in [partial_path(&dest, &entry.sha256), dest] {
            match fs::remove_file(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => {
                    problem = Some(format!("could not delete {}: {e}", path.display()));
                }
                _ => {}
            }
        }
        if was_running {
            self.start(app);
        }
        problem.map_or(Ok(()), Err)
    }
}

#[tauri::command]
pub fn list_models(models: tauri::State<'_, Models>) -> ModelsInfo {
    models.info()
}

#[tauri::command]
pub fn download_model(
    app: tauri::AppHandle,
    id: String,
    models: tauri::State<'_, Models>,
) -> Result<(), String> {
    models.download(&app, &id)
}

#[tauri::command]
pub fn cancel_download(id: String, models: tauri::State<'_, Models>) {
    models.cancel(&id);
}

#[tauri::command]
pub fn set_mobile_data(allowed: bool, models: tauri::State<'_, Models>) -> Result<(), String> {
    models.set_mobile_data(allowed)
}

#[tauri::command]
pub fn use_model(
    app: tauri::AppHandle,
    id: String,
    models: tauri::State<'_, Models>,
) -> Result<(), String> {
    models.choose(&app, &id)
}

#[tauri::command]
pub fn delete_model(
    app: tauri::AppHandle,
    id: String,
    models: tauri::State<'_, Models>,
) -> Result<(), String> {
    models.delete(&app, &id)
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    /// A server for one file that answers `Range` requests like Hugging Face's, or
    /// ignores them. It stops after `requests` requests.
    fn serve(body: Vec<u8>, ranges: bool, requests: usize) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(requests) {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut from = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim().is_empty() {
                        break;
                    }
                    let lower = line.to_ascii_lowercase();
                    if let Some(rest) = lower.strip_prefix("range: bytes=") {
                        from = rest.trim().trim_end_matches('-').parse().unwrap();
                    }
                }
                let head = if !ranges || from == 0 {
                    format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n", body.len())
                } else if from >= body.len() {
                    "HTTP/1.1 416 Range Not Satisfiable\r\ncontent-length: 0\r\n\r\n".to_string()
                } else {
                    format!(
                        "HTTP/1.1 206 Partial Content\r\ncontent-range: bytes {from}-{}/{}\r\ncontent-length: {}\r\n\r\n",
                        body.len() - 1,
                        body.len(),
                        body.len() - from
                    )
                };
                let start = if ranges { from.min(body.len()) } else { 0 };
                // A client that stops early hangs up mid-body; the next request still comes.
                let _ = stream.write_all(head.as_bytes());
                if !head.contains(" 416 ") {
                    let _ = stream.write_all(&body[start..]);
                }
            }
        });
        format!("http://{address}/model.gguf")
    }

    fn body() -> (Vec<u8>, String) {
        let body: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        let digest = Sha256::digest(&body)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        (body, digest)
    }

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pr3-models-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fetch(url: &str, dest: &Path, body: &[u8], sha256: &str) -> Result<u64, DownloadError> {
        let mut last = 0;
        download(
            &agent_for(false),
            url,
            dest,
            body.len() as u64,
            sha256,
            &|| false,
            &mut |bytes| last = bytes,
        )
        .map(|()| last)
    }

    #[test]
    fn a_download_gets_its_name_only_when_verified() {
        let (body, sha256) = body();
        let dir = folder("whole");
        let dest = dir.join("model.gguf");
        let url = serve(body.clone(), true, 1);
        assert_eq!(
            fetch(&url, &dest, &body, &sha256).unwrap(),
            body.len() as u64
        );
        assert_eq!(fs::read(&dest).unwrap(), body);
        assert!(!partial_path(&dest, &sha256).exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_partial_download_resumes_where_it_stopped() {
        let (body, sha256) = body();
        let dir = folder("resume");
        let dest = dir.join("model.gguf");
        fs::write(partial_path(&dest, &sha256), &body[..1_234_567]).unwrap();
        fetch(&serve(body.clone(), true, 1), &dest, &body, &sha256).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), body);

        // A whole partial file only needs checking.
        fs::remove_file(&dest).unwrap();
        fs::write(partial_path(&dest, &sha256), &body).unwrap();
        fetch(&serve(body.clone(), true, 1), &dest, &body, &sha256).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), body);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_server_that_cannot_resume_sends_it_all_again() {
        let (body, sha256) = body();
        let dir = folder("restart");
        let dest = dir.join("model.gguf");
        fs::write(partial_path(&dest, &sha256), vec![7; 1000]).unwrap();
        fetch(&serve(body.clone(), false, 1), &dest, &body, &sha256).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), body);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_that_is_not_the_pinned_one_is_removed() {
        let (whole, sha256) = body();
        let dir = folder("mismatch");
        let dest = dir.join("model.gguf");
        let wrong = "0".repeat(64);
        let result = fetch(&serve(whole.clone(), true, 1), &dest, &whole, &wrong);
        assert!(matches!(result, Err(DownloadError::Mismatch)), "{result:?}");
        assert!(!dest.exists() && !partial_path(&dest, &wrong).exists());

        // A partial file that resumes into the wrong file is caught too.
        fs::write(partial_path(&dest, &sha256), vec![7; 1000]).unwrap();
        let result = fetch(&serve(whole.clone(), true, 1), &dest, &whole, &sha256);
        assert!(matches!(result, Err(DownloadError::Mismatch)), "{result:?}");
        assert!(!dest.exists() && !partial_path(&dest, &sha256).exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_cancelled_download_keeps_what_arrived() {
        let (body, sha256) = body();
        let dir = folder("cancel");
        let dest = dir.join("model.gguf");
        let cancel = AtomicBool::new(false);
        let result = download(
            &agent_for(false),
            &serve(body.clone(), true, 1),
            &dest,
            body.len() as u64,
            &sha256,
            &|| cancel.load(Ordering::Relaxed),
            &mut |bytes| {
                if bytes > 0 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        );
        assert!(
            matches!(result, Err(DownloadError::Cancelled)),
            "{result:?}"
        );
        assert!(!dest.exists());
        let kept = fs::metadata(partial_path(&dest, &sha256)).unwrap().len();
        assert!(kept > 0 && kept < body.len() as u64);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn only_https_reaches_a_model_repository() {
        let result = download(
            &agent(),
            "http://127.0.0.1:9/model.gguf",
            &std::env::temp_dir().join("pr3-never.gguf"),
            1,
            &"0".repeat(64),
            &|| false,
            &mut |_| {},
        );
        assert!(matches!(result, Err(DownloadError::Http(_))), "{result:?}");
    }

    /// Makes the network free again after a moment, as if the phone found Wi-Fi.
    fn wifi_returns(costly: &Arc<AtomicBool>) {
        let costly = Arc::clone(costly);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            costly.store(false, Ordering::Relaxed);
        });
    }

    #[test]
    fn on_mobile_data_a_download_waits_for_wifi() {
        let (body, sha256) = body();
        let dir = folder("wait");
        let dest = dir.join("model.gguf");
        let costly = Arc::new(AtomicBool::new(true));
        let mut events = Vec::new();
        wifi_returns(&costly);
        download_when_free(
            &agent_for(false),
            &serve(body.clone(), true, 1),
            &dest,
            body.len() as u64,
            &sha256,
            &AtomicBool::new(false),
            &|| costly.load(Ordering::Relaxed),
            &mut |waiting| events.push(waiting),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(events, [true, false]);
        assert_eq!(fs::read(&dest).unwrap(), body);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_download_pauses_on_mobile_data_and_resumes_on_wifi() {
        let (body, sha256) = body();
        let dir = folder("pause");
        let dest = dir.join("model.gguf");
        let costly = Arc::new(AtomicBool::new(false));
        let mut events = Vec::new();
        let mut paused_at = 0;
        let left_wifi = Cell::new(false);
        download_when_free(
            &agent_for(false),
            &serve(body.clone(), true, 2),
            &dest,
            body.len() as u64,
            &sha256,
            &AtomicBool::new(false),
            &|| costly.load(Ordering::Relaxed),
            &mut |waiting| {
                if waiting {
                    paused_at = fs::metadata(partial_path(&dest, &sha256)).unwrap().len();
                    wifi_returns(&costly);
                }
                events.push(waiting);
            },
            &mut |bytes| {
                // The phone leaves Wi-Fi once the first megabyte is in.
                if bytes > 0 && !left_wifi.replace(true) {
                    costly.store(true, Ordering::Relaxed);
                }
            },
        )
        .unwrap();
        assert_eq!(events, [true, false]);
        assert!(
            paused_at > 0 && paused_at < body.len() as u64,
            "{paused_at}"
        );
        assert_eq!(fs::read(&dest).unwrap(), body);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_download_waiting_for_wifi_can_be_cancelled() {
        let (_, sha256) = body();
        let dir = folder("wait-cancel");
        let dest = dir.join("model.gguf");
        let cancel = Arc::new(AtomicBool::new(false));
        let later = Arc::clone(&cancel);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            later.store(true, Ordering::Relaxed);
        });
        let result = download_when_free(
            &agent_for(false),
            "http://127.0.0.1:9/never.gguf",
            &dest,
            1,
            &sha256,
            &cancel,
            &|| true,
            &mut |_| {},
            &mut |_| {},
        );
        assert!(
            matches!(result, Err(DownloadError::Cancelled)),
            "{result:?}"
        );
        assert!(!partial_path(&dest, &sha256).exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_model_must_leave_room_to_run() {
        let config = Config::built_in().unwrap();
        let size = |id: &str| config.models[id].size;
        let phone = |gb| Device::Mobile {
            memory: Some(gb * GB),
        };
        let computer = |gb| Device::Desktop {
            memory: Some(gb * GB),
        };
        // The config's picks fit the devices it picks them for.
        assert!(fits(phone(8), size("qwen3.5-4b")));
        assert!(fits(phone(4), size("qwen3.5-2b")));
        assert!(fits(computer(32), size("qwen3.8-27b")));
        assert!(!fits(phone(12), size("qwen3.8-27b")));
        assert!(!fits(phone(4), size("qwen3.5-4b")));
        assert!(!fits(computer(16), size("qwen3.8-27b")));
        assert!(fits(
            Device::Desktop { memory: None },
            size("qwen3.8-27b-q8")
        ));
    }
}
