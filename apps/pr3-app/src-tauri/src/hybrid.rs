//! The private hybrid in the app (Roadmap §6.3): a computer lends its model to the
//! phones it paired with, and a paired phone uses it, answering on its own whenever the
//! computer cannot be reached.
//!
//! Only generation crosses: the phone keeps the conversation, memory, tools and the whole
//! trusted side. A model on the computer can propose sending an email; the phone
//! validates the proposal, shows the card, asks for Face ID and sends — or not.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use pr3_core::InferenceRequest;
use pr3_hybrid::{DeviceKey, Generator, HybridError, Node, NodeClient, Peer};
use pr3_protocol::Generated;
use serde::Serialize;
use tauri::Manager;

use crate::config;
use crate::host::{Backend, Generation, Host};

const NODE_FILE: &str = "node.json";
const KEY_FILE: &str = "device.key";

/// This device's name, as the other side shows it.
pub fn device_name() -> String {
    if cfg!(target_os = "ios") {
        "iPhone".to_string()
    } else if cfg!(target_os = "android") {
        "Android phone".to_string()
    } else {
        computer_name()
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn computer_name() -> String {
    let host = gethostname::gethostname().to_string_lossy().into_owned();
    let name = host.trim_end_matches(".local").replace('-', " ");
    if name.trim().is_empty() {
        "computer".to_string()
    } else {
        name
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn computer_name() -> String {
    "computer".to_string()
}

/// On a computer: lends the app's current model to paired phones.
struct HostGenerator {
    app: tauri::AppHandle,
}

impl Generator for HostGenerator {
    fn model(&self) -> String {
        self.app.state::<Host>().backend_label()
    }

    fn generate(
        &self,
        request: &InferenceRequest,
        on_token: &mut dyn FnMut(&str) -> bool,
    ) -> Result<Generated, String> {
        let backend = self.app.state::<Host>().backend();
        let generation = backend.generate_until(request, on_token)?;
        Ok(Generated {
            content: generation.content,
            tool_calls: generation.tool_calls,
            info: generation.info,
        })
    }
}

/// Starts the node if the config asks for it. Returns it for the pairing commands.
pub fn start_node(
    app: &tauri::AppHandle,
    dir: &Path,
    settings: &config::Node,
) -> Option<Arc<Node>> {
    if !settings.serve {
        return None;
    }
    let generator = Arc::new(HostGenerator { app: app.clone() });
    let started = Node::open(dir, &device_name(), generator).and_then(|node| {
        let bound = node.serve(&settings.listen)?;
        Ok((node, bound))
    });
    match started {
        Ok((node, bound)) => {
            eprintln!(
                "PR3 node: {} lends its model on {bound} to {} paired phone(s)",
                node.name(),
                node.phones().len()
            );
            // Debug builds only: a pairing code at startup, for testing pairing from a
            // simulator or emulator without clicking through the computer's window.
            #[cfg(debug_assertions)]
            if std::env::var_os("PR3_PAIR_ON_START").is_some()
                && let Ok(code) = node.start_pairing()
            {
                eprintln!("PR3 node: pairing code {}", code.code);
            }
            Some(node)
        }
        Err(e) => {
            eprintln!("PR3 node: not started: {e}");
            None
        }
    }
}

/// On a phone: the computer it paired with, if any.
#[derive(Clone, Default)]
pub struct Link(Arc<RwLock<Option<Arc<NodeClient>>>>);

impl Link {
    /// Restores the pairing stored in `dir`.
    pub fn open(dir: &Path) -> Self {
        let link = Self::default();
        let restored = std::fs::read_to_string(dir.join(NODE_FILE))
            .ok()
            .and_then(|text| serde_json::from_str::<Peer>(&text).ok())
            .and_then(|node| {
                let key = DeviceKey::open(&dir.join(KEY_FILE)).ok()?;
                Some(NodeClient::new(key, node, &device_name()))
            });
        if let Some(client) = restored {
            eprintln!("PR3: paired with {}", client.node().name);
            link.set(Some(Arc::new(client)));
        }
        link
    }

    fn get(&self) -> Option<Arc<NodeClient>> {
        self.0.read().ok().and_then(|c| c.clone())
    }

    fn set(&self, client: Option<Arc<NodeClient>>) {
        if let Ok(mut current) = self.0.write() {
            *current = client;
        }
    }

    /// Pairs with the computer at `address` and stores it.
    pub fn pair(&self, dir: &Path, address: &str, code: &str) -> Result<Peer, String> {
        let key = DeviceKey::open(&dir.join(KEY_FILE)).map_err(|e| e.to_string())?;
        let node =
            pr3_hybrid::pair(address, code, &key, &device_name()).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(&node).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(NODE_FILE), text).map_err(|e| e.to_string())?;
        self.set(Some(Arc::new(NodeClient::new(
            key,
            node.clone(),
            &device_name(),
        ))));
        Ok(node)
    }

    /// Forgets the computer. It keeps this phone on its list until removed there too,
    /// but this phone no longer sends it anything.
    pub fn unpair(&self, dir: &Path) -> Result<(), String> {
        match std::fs::remove_file(dir.join(NODE_FILE)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
            _ => {}
        }
        self.set(None);
        Ok(())
    }

    pub fn node(&self) -> Option<Peer> {
        self.get().map(|c| c.node().clone())
    }
}

/// On a phone: asks the paired computer first, and this phone's own model when the
/// computer cannot be reached or fails.
pub struct Delegating {
    link: Link,
    local: Arc<dyn Backend>,
    cancel: AtomicBool,
    /// The computer's model, once it has answered.
    remote_model: RwLock<Option<String>>,
    /// Whether the last attempt to reach the computer failed.
    unreachable: AtomicBool,
}

impl Delegating {
    pub fn new(link: Link, local: Arc<dyn Backend>) -> Self {
        Self {
            link,
            local,
            cancel: AtomicBool::new(false),
            remote_model: RwLock::new(None),
            unreachable: AtomicBool::new(false),
        }
    }
}

impl Backend for Delegating {
    fn label(&self) -> String {
        let local = self.local.label();
        match self.link.get() {
            Some(client) => {
                let model = self.remote_model.read().ok().and_then(|m| m.clone());
                let node = &client.node().name;
                if self.unreachable.load(Ordering::Relaxed) {
                    return format!("{local} here ({node} unreachable)");
                }
                match model {
                    Some(model) => format!("{model} on {node}, else {local}"),
                    None => format!("{node}, else {local}"),
                }
            }
            None => local,
        }
    }

    fn generate(
        &self,
        request: &InferenceRequest,
        on_token: &mut dyn FnMut(&str),
    ) -> Result<Generation, String> {
        if let Some(client) = self.link.get() {
            self.cancel.store(false, Ordering::Relaxed);
            match client.generate(request, on_token, &self.cancel) {
                Ok(generated) => {
                    self.unreachable.store(false, Ordering::Relaxed);
                    if let Ok(mut model) = self.remote_model.write() {
                        *model = Some(generated.info.model.clone());
                    }
                    return Ok(Generation {
                        content: generated.content,
                        tool_calls: generated.tool_calls,
                        info: generated.info,
                    });
                }
                Err(HybridError::Cancelled) => return Err("cancelled".to_string()),
                // Unreachable, refused or failed: this phone answers instead.
                Err(e) => {
                    self.unreachable.store(true, Ordering::Relaxed);
                    eprintln!("PR3: {e}; answering on this phone");
                }
            }
        }
        self.local.generate(request, on_token)
    }

    fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.local.stop();
    }
}

/// What the devices panel shows.
#[derive(Serialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum DevicesInfo {
    /// A computer lending its model.
    Node {
        name: String,
        listen: String,
        phones: Vec<PeerInfo>,
    },
    /// A phone, paired or not.
    Phone { node: Option<PeerInfo> },
    /// Neither: no node configured on this computer.
    Off,
}

#[derive(Serialize)]
pub struct PeerInfo {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub paired_at: u64,
}

impl From<&Peer> for PeerInfo {
    fn from(peer: &Peer) -> Self {
        Self {
            id: peer.id(),
            name: peer.name.clone(),
            address: peer.address.clone(),
            paired_at: peer.paired_at,
        }
    }
}
