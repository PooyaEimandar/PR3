//! The node's side: the user's computer, lending its model to the phones it paired with.

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pr3_core::InferenceRequest;
use pr3_protocol::{FromNode, Generated, ToNode, VERSION};

use crate::channel::read_frame;
use crate::keys::{DeviceKey, Peer, load_peers, save_peers};
use crate::pairing::{PAIR_HELLO, SESSION_HELLO, accept_session, confirm_pairing, pair_as_node};
use crate::{HybridError, to_hex};

/// How long a pairing code stays valid.
const PAIRING_WINDOW: Duration = Duration::from_secs(120);
/// Connections handled at once; more wait in the kernel's queue or are dropped.
const MAX_CONNECTIONS: usize = 8;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// A paired phone that goes silent this long is disconnected.
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);

/// What a node lends: its model.
pub trait Generator: Send + Sync {
    /// The model's name, shown on the phone.
    fn model(&self) -> String;

    /// Answers `request`; `on_token` gets visible text as it is written and returns
    /// `false` to stop early (the phone cancelled or went away).
    fn generate(
        &self,
        request: &InferenceRequest,
        on_token: &mut dyn FnMut(&str) -> bool,
    ) -> Result<Generated, String>;
}

/// A code to type on the phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingCode {
    pub code: String,
    pub expires_in: Duration,
}

pub struct Node {
    name: String,
    key: DeviceKey,
    phones_path: PathBuf,
    phones: Mutex<Vec<Peer>>,
    pairing: Mutex<Option<(String, Instant)>>,
    generator: Arc<dyn Generator>,
    connections: AtomicUsize,
}

impl Node {
    /// Opens the node's key and its list of paired phones in `dir`.
    pub fn open(
        dir: &Path,
        name: &str,
        generator: Arc<dyn Generator>,
    ) -> std::io::Result<Arc<Self>> {
        let key = DeviceKey::open(&dir.join("device.key"))?;
        let phones_path = dir.join("phones.json");
        let phones = load_peers(&phones_path)?;
        Ok(Arc::new(Self {
            name: crate::keys::clean_name(name),
            key,
            phones_path,
            phones: Mutex::new(phones),
            pairing: Mutex::new(None),
            generator,
            connections: AtomicUsize::new(0),
        }))
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Listens on `address` (such as `127.0.0.1:7313`) on a background thread. Returns
    /// the address actually bound.
    pub fn serve(self: &Arc<Self>, address: &str) -> std::io::Result<SocketAddr> {
        let listener = TcpListener::bind(address)?;
        let bound = listener.local_addr()?;
        let node = Arc::clone(self);
        std::thread::Builder::new()
            .name("pr3-node".to_string())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    if node.connections.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                        node.connections.fetch_sub(1, Ordering::SeqCst);
                        continue;
                    }
                    let worker = Arc::clone(&node);
                    let spawned = std::thread::Builder::new()
                        .name("pr3-node-connection".to_string())
                        .spawn(move || {
                            if let Err(e) = worker.handle(&stream) {
                                eprintln!("PR3 node: connection ended: {e}");
                            }
                            worker.connections.fetch_sub(1, Ordering::SeqCst);
                        });
                    if spawned.is_err() {
                        node.connections.fetch_sub(1, Ordering::SeqCst);
                    }
                }
            })?;
        Ok(bound)
    }

    /// A fresh six-digit code, valid for one pairing attempt within two minutes. Any
    /// earlier code stops working.
    pub fn start_pairing(&self) -> std::io::Result<PairingCode> {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
        let code = format!("{:06}", u32::from_le_bytes(bytes) % 1_000_000);
        if let Ok(mut pairing) = self.pairing.lock() {
            *pairing = Some((code.clone(), Instant::now() + PAIRING_WINDOW));
        }
        Ok(PairingCode {
            code,
            expires_in: PAIRING_WINDOW,
        })
    }

    /// The phones paired with this node.
    pub fn phones(&self) -> Vec<Peer> {
        self.phones.lock().map(|p| p.clone()).unwrap_or_default()
    }

    /// Forgets a phone: its sessions are refused from now on.
    pub fn forget(&self, id: &str) -> std::io::Result<bool> {
        let mut phones = self
            .phones
            .lock()
            .map_err(|_| std::io::Error::other("phone list unavailable"))?;
        let before = phones.len();
        phones.retain(|p| p.id() != id);
        let removed = phones.len() != before;
        if removed {
            save_peers(&self.phones_path, &phones)?;
        }
        Ok(removed)
    }

    fn handle(&self, stream: &TcpStream) -> Result<(), HybridError> {
        stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
        stream.set_nodelay(true)?;
        let hello = read_frame(stream)?;
        if hello == PAIR_HELLO {
            self.pair(stream)
        } else if hello == SESSION_HELLO {
            self.session(stream)
        } else {
            Err(HybridError::Protocol("unknown connection".to_string()))
        }
    }

    fn pair(&self, stream: &TcpStream) -> Result<(), HybridError> {
        // One attempt per code: taken before the phone proves anything, so guessing
        // means asking the user for a new code each time.
        let code = self
            .pairing
            .lock()
            .ok()
            .and_then(|mut p| p.take())
            .filter(|(_, until)| Instant::now() < *until)
            .map(|(code, _)| code)
            .ok_or_else(|| HybridError::Refused("no pairing in progress".to_string()))?;
        let (name, phone_key, channel) = pair_as_node(stream, &self.key, &code)?;
        let phone = Peer::new(&name, &phone_key, None);
        {
            let mut phones = self
                .phones
                .lock()
                .map_err(|_| HybridError::Protocol("phone list unavailable".to_string()))?;
            phones.retain(|p| p.public_key != phone.public_key);
            phones.push(phone.clone());
            save_peers(&self.phones_path, &phones)?;
        }
        confirm_pairing(&channel, &self.name)?;
        eprintln!("PR3 node: paired with {} ({})", phone.name, phone.id());
        Ok(())
    }

    fn session(&self, stream: &TcpStream) -> Result<(), HybridError> {
        let known = |key: &[u8; 32]| {
            let hex = to_hex(key);
            self.phones
                .lock()
                .is_ok_and(|phones| phones.iter().any(|p| p.public_key == hex))
        };
        let (phone_key, channel) = accept_session(stream, &self.key, known)?;
        let phone = self
            .phones()
            .into_iter()
            .find(|p| p.public_key == to_hex(&phone_key))
            .map_or_else(|| "a phone".to_string(), |p| p.name);
        stream.set_read_timeout(Some(IDLE_TIMEOUT))?;
        match channel.recv::<ToNode>()? {
            // The phone compares versions and hangs up on a mismatch.
            ToNode::Hello { .. } => channel.send(&FromNode::Hello {
                version: VERSION,
                node: self.name.clone(),
                model: self.generator.model(),
            })?,
            _ => return Err(HybridError::Protocol("expected hello".to_string())),
        }
        // One request per session; a phone that only asked for the model hangs up here.
        let (id, request) = match channel.recv::<ToNode>() {
            Ok(ToNode::Generate { id, request }) => (id, request),
            Ok(_) => return Err(HybridError::Protocol("expected a request".to_string())),
            Err(_) => return Ok(()),
        };
        let cancelled = AtomicBool::new(false);
        std::thread::scope(|scope| {
            // Watches for Cancel, or the phone hanging up, while the model runs.
            scope.spawn(|| {
                while let Ok(message) = channel.recv::<ToNode>() {
                    if matches!(message, ToNode::Cancel { id: c } if c == id) {
                        break;
                    }
                }
                cancelled.store(true, Ordering::Relaxed);
            });
            let mut on_token = |text: &str| {
                !cancelled.load(Ordering::Relaxed)
                    && channel
                        .send(&FromNode::Token {
                            id,
                            text: text.to_string(),
                        })
                        .is_ok()
            };
            let started = Instant::now();
            let answer = match self.generator.generate(&request, &mut on_token) {
                _ if cancelled.load(Ordering::Relaxed) => FromNode::Failed {
                    id,
                    reason: "cancelled".to_string(),
                },
                Ok(generated) => FromNode::Done { id, generated },
                Err(reason) => FromNode::Failed { id, reason },
            };
            match &answer {
                FromNode::Done { generated, .. } => eprintln!(
                    "PR3 node: answered {phone} with {} ({} tokens, {:.1?})",
                    generated.info.model,
                    generated.info.completion_tokens,
                    started.elapsed()
                ),
                FromNode::Failed { reason, .. } => {
                    eprintln!("PR3 node: did not answer {phone}: {reason}");
                }
                _ => {}
            }
            let _ = channel.send(&answer);
            // Ends the watcher's read.
            channel.close();
        });
        Ok(())
    }
}
