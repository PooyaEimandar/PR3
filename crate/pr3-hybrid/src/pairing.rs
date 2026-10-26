//! The two handshakes: pairing with a code, and sessions between paired devices.
//!
//! Every connection opens with a plaintext frame naming which one follows.

use std::net::TcpStream;

use snow::{Builder, HandshakeState};
use spake2::{Ed25519Group, Identity, Password, Spake2};

use crate::HybridError;
use crate::channel::{Channel, NOISE_MAX, read_frame, write_frame};
use crate::keys::{DeviceKey, clean_name};

pub(crate) const PAIR_HELLO: &[u8] = b"pr3/0 pair";
pub(crate) const SESSION_HELLO: &[u8] = b"pr3/0 session";

const PAIR_NOISE: &str = "Noise_XXpsk3_25519_ChaChaPoly_BLAKE2s";
pub(crate) const SESSION_NOISE: &str = "Noise_IK_25519_ChaChaPoly_BLAKE2s";
const PAIR_IDENTITY: &[u8] = b"pr3-pairing/0";
/// The node's last pairing message: it has stored the phone.
const PAIRED: &[u8] = b"paired";

fn builder<'a>(pattern: &str) -> Result<Builder<'a>, HybridError> {
    let params = pattern
        .parse()
        .map_err(|e: snow::Error| HybridError::Protocol(e.to_string()))?;
    Ok(Builder::new(params))
}

fn write_handshake(
    stream: &TcpStream,
    state: &mut HandshakeState,
    payload: &[u8],
) -> Result<(), HybridError> {
    let mut message = vec![0u8; NOISE_MAX];
    let n = state.write_message(payload, &mut message)?;
    write_frame(stream, &message[..n])?;
    Ok(())
}

fn read_handshake(stream: &TcpStream, state: &mut HandshakeState) -> Result<Vec<u8>, HybridError> {
    let message = read_frame(stream)?;
    let mut payload = vec![0u8; message.len()];
    let n = state.read_message(&message, &mut payload)?;
    payload.truncate(n);
    Ok(payload)
}

/// SPAKE2 on the code: both sides end with the same 32-byte key only if they used the
/// same code, and an eavesdropper learns nothing that lets them test guesses offline.
/// Returns the key and both SPAKE2 messages, which the Noise handshake then binds.
fn spake(
    stream: &TcpStream,
    code: &str,
    phone_first: bool,
) -> Result<([u8; 32], Vec<u8>), HybridError> {
    let (state, ours) = Spake2::<Ed25519Group>::start_symmetric(
        &Password::new(code.as_bytes()),
        &Identity::new(PAIR_IDENTITY),
    );
    write_frame(stream, &ours)?;
    let theirs = read_frame(stream)?;
    let key = state
        .finish(&theirs)
        .map_err(|e| HybridError::Protocol(format!("pairing: {e:?}")))?;
    let key: [u8; 32] = key
        .as_slice()
        .try_into()
        .map_err(|_| HybridError::Protocol("pairing key has the wrong length".to_string()))?;
    let (first, second) = if phone_first {
        (ours, theirs)
    } else {
        (theirs, ours)
    };
    let mut prologue = PAIR_NOISE.as_bytes().to_vec();
    prologue.extend_from_slice(&first);
    prologue.extend_from_slice(&second);
    Ok((key, prologue))
}

/// The phone's side of pairing. Returns the node's name and public key.
pub(crate) fn pair_as_phone(
    stream: &TcpStream,
    key: &DeviceKey,
    code: &str,
    device: &str,
) -> Result<(String, [u8; 32]), HybridError> {
    write_frame(stream, PAIR_HELLO)?;
    let (psk, prologue) = spake(stream, code, true)?;
    let mut state = builder(PAIR_NOISE)?
        .local_private_key(&key.private)?
        .psk(3, &psk)?
        .prologue(&prologue)?
        .build_initiator()?;
    write_handshake(stream, &mut state, &[])?;
    read_handshake(stream, &mut state)?;
    // The third message carries the pre-shared key: only a node that used the same code
    // can read it, or this name.
    write_handshake(stream, &mut state, clean_name(device).as_bytes())?;
    let node_key = remote_key(&state)?;
    let channel = Channel::new(stream.try_clone()?, state.into_transport_mode()?);
    // A wrong code shows up here: the node cannot read message three and hangs up.
    let paired: (String, String) = channel.recv().map_err(|_| {
        HybridError::Refused("the code was wrong or has expired; start pairing again".into())
    })?;
    if paired.0.as_bytes() != PAIRED {
        return Err(HybridError::Protocol(
            "unexpected pairing answer".to_string(),
        ));
    }
    Ok((clean_name(&paired.1), node_key))
}

/// The node's side of pairing, after it read [`PAIR_HELLO`]. Returns the phone's name and
/// public key, and the channel on which to confirm.
pub(crate) fn pair_as_node(
    stream: &TcpStream,
    key: &DeviceKey,
    code: &str,
) -> Result<(String, [u8; 32], Channel), HybridError> {
    let (psk, prologue) = spake(stream, code, false)?;
    let mut state = builder(PAIR_NOISE)?
        .local_private_key(&key.private)?
        .psk(3, &psk)?
        .prologue(&prologue)?
        .build_responder()?;
    read_handshake(stream, &mut state)?;
    write_handshake(stream, &mut state, &[])?;
    let name = read_handshake(stream, &mut state)
        .map_err(|_| HybridError::Refused("wrong pairing code".to_string()))?;
    let phone_key = remote_key(&state)?;
    let channel = Channel::new(stream.try_clone()?, state.into_transport_mode()?);
    Ok((
        clean_name(&String::from_utf8_lossy(&name)),
        phone_key,
        channel,
    ))
}

/// Tells the phone it is stored, with the node's name.
pub(crate) fn confirm_pairing(channel: &Channel, node: &str) -> Result<(), HybridError> {
    let answer = (
        String::from_utf8_lossy(PAIRED).into_owned(),
        clean_name(node),
    );
    channel.send(&answer)?;
    Ok(())
}

/// The phone opens a session with a node it paired with.
pub(crate) fn open_session(
    stream: &TcpStream,
    key: &DeviceKey,
    node_key: &[u8; 32],
) -> Result<Channel, HybridError> {
    write_frame(stream, SESSION_HELLO)?;
    let mut state = builder(SESSION_NOISE)?
        .local_private_key(&key.private)?
        .remote_public_key(node_key)?
        .build_initiator()?;
    write_handshake(stream, &mut state, &[])?;
    // A node that does not know this phone hangs up instead of answering.
    read_handshake(stream, &mut state).map_err(|e| match e {
        HybridError::Unreachable(_) => {
            HybridError::Refused("the node does not know this device; pair again".into())
        }
        other => other,
    })?;
    Ok(Channel::new(
        stream.try_clone()?,
        state.into_transport_mode()?,
    ))
}

/// The node's side of a session, after it read [`SESSION_HELLO`]: authenticates the
/// phone and asks `known` whether it is paired. An unknown phone gets no answer.
pub(crate) fn accept_session(
    stream: &TcpStream,
    key: &DeviceKey,
    known: impl Fn(&[u8; 32]) -> bool,
) -> Result<([u8; 32], Channel), HybridError> {
    let mut state = builder(SESSION_NOISE)?
        .local_private_key(&key.private)?
        .build_responder()?;
    read_handshake(stream, &mut state)?;
    let phone = remote_key(&state)?;
    if !known(&phone) {
        return Err(HybridError::Refused("unknown device".to_string()));
    }
    write_handshake(stream, &mut state, &[])?;
    Ok((
        phone,
        Channel::new(stream.try_clone()?, state.into_transport_mode()?),
    ))
}

fn remote_key(state: &HandshakeState) -> Result<[u8; 32], HybridError> {
    state
        .get_remote_static()
        .and_then(|k| k.try_into().ok())
        .ok_or_else(|| HybridError::Protocol("no remote key".to_string()))
}
