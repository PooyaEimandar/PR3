//! The phone's side: pairing with a node, and asking it to generate.

use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use pr3_core::InferenceRequest;
use pr3_protocol::{FromNode, Generated, ToNode, VERSION};

use crate::channel::Channel;
use crate::keys::{DeviceKey, Peer};
use crate::pairing::{open_session, pair_as_phone};
use crate::{DEFAULT_PORT, HybridError};

/// How long to try reaching the node before answering on the phone instead.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// A handshake step slower than this means something is wrong.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// The longest silence while the node generates: its first token can take a while on a
/// long conversation, later ones come quickly.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(120);

/// `host` or `host:port`.
fn connect(address: &str) -> Result<TcpStream, HybridError> {
    let with_port = if address
        .rsplit_once(':')
        .is_some_and(|(_, p)| p.parse::<u16>().is_ok())
    {
        address.to_string()
    } else {
        format!("{address}:{DEFAULT_PORT}")
    };
    let targets: Vec<SocketAddr> = with_port.to_socket_addrs()?.collect();
    let mut last = None;
    for target in targets {
        match TcpStream::connect_timeout(&target, CONNECT_TIMEOUT) {
            Ok(stream) => {
                stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
                stream.set_nodelay(true)?;
                return Ok(stream);
            }
            Err(e) => last = Some(e),
        }
    }
    Err(HybridError::Unreachable(last.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no such address")
    })))
}

/// Pairs this device with the node at `address`, using the code the node shows. Returns
/// the node, with its address, for the caller to store.
pub fn pair(address: &str, code: &str, key: &DeviceKey, device: &str) -> Result<Peer, HybridError> {
    let code: String = code.chars().filter(char::is_ascii_digit).collect();
    if code.len() != 6 {
        return Err(HybridError::Refused("the code has six digits".to_string()));
    }
    let stream = connect(address)?;
    let (name, node_key) = pair_as_phone(&stream, key, &code, device)?;
    Ok(Peer::new(
        &name,
        &node_key,
        Some(address.trim().to_string()),
    ))
}

/// A paired node, ready to generate for this device.
pub struct NodeClient {
    key: DeviceKey,
    node: Peer,
    device: String,
}

impl NodeClient {
    pub fn new(key: DeviceKey, node: Peer, device: &str) -> Self {
        Self {
            key,
            node,
            device: device.to_string(),
        }
    }

    pub fn node(&self) -> &Peer {
        &self.node
    }

    /// Connects, authenticates both ways and says hello. Returns the channel and the
    /// model the node lends.
    fn session(&self) -> Result<(Channel, String), HybridError> {
        let address = self
            .node
            .address
            .as_deref()
            .ok_or_else(|| HybridError::Refused("the node has no address".to_string()))?;
        let node_key = self
            .node
            .key()
            .ok_or_else(|| HybridError::Protocol("damaged node key".to_string()))?;
        let stream = connect(address)?;
        let channel = open_session(&stream, &self.key, &node_key)?;
        channel.send(&ToNode::Hello {
            version: VERSION,
            device: self.device.clone(),
        })?;
        let model = match channel.recv::<FromNode>()? {
            FromNode::Hello { version, model, .. } if version == VERSION => model,
            FromNode::Hello { version, .. } => {
                return Err(HybridError::Refused(format!(
                    "the node speaks protocol {version}, this device {VERSION}"
                )));
            }
            _ => return Err(HybridError::Protocol("expected hello".to_string())),
        };
        stream.set_read_timeout(Some(ANSWER_TIMEOUT))?;
        Ok((channel, model))
    }

    /// The model the node lends, asked over a fresh session.
    pub fn model(&self) -> Result<String, HybridError> {
        let (channel, model) = self.session()?;
        channel.close();
        Ok(model)
    }

    /// Runs `request` on the node, streaming its text to `on_token`. Setting `cancel`
    /// asks the node to stop; the call then returns [`HybridError::Cancelled`].
    pub fn generate(
        &self,
        request: &InferenceRequest,
        on_token: &mut dyn FnMut(&str),
        cancel: &AtomicBool,
    ) -> Result<Generated, HybridError> {
        const ID: u64 = 1;
        let (channel, _) = self.session()?;
        channel.send(&ToNode::Generate {
            id: ID,
            request: request.clone(),
        })?;
        let finished = AtomicBool::new(false);
        let result = std::thread::scope(|scope| {
            // Passes a cancellation on while this thread waits for the node.
            scope.spawn(|| {
                while !finished.load(Ordering::Relaxed) {
                    if cancel.load(Ordering::Relaxed) {
                        let _ = channel.send(&ToNode::Cancel { id: ID });
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            });
            let result = loop {
                match channel.recv::<FromNode>() {
                    Ok(FromNode::Token { id: ID, text }) => on_token(&text),
                    Ok(FromNode::Done { id: ID, generated }) => break Ok(generated),
                    Ok(FromNode::Failed { .. }) if cancel.load(Ordering::Relaxed) => {
                        break Err(HybridError::Cancelled);
                    }
                    Ok(FromNode::Failed { id: ID, reason }) => {
                        break Err(HybridError::NodeFailed(reason));
                    }
                    Ok(_) => break Err(HybridError::Protocol("unexpected message".to_string())),
                    Err(e) => break Err(HybridError::Unreachable(e)),
                }
            };
            finished.store(true, Ordering::Relaxed);
            result
        });
        channel.close();
        result
    }
}
