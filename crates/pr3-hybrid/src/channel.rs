//! Frames over TCP, and the encrypted channel a finished handshake gives.
//!
//! A frame is a 4-byte big-endian length and that many bytes, at most one Noise message
//! (65535 bytes). A protocol message larger than one Noise message is split into chunks,
//! each starting with a byte that says whether more follow.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::Mutex;

use pr3_protocol::{MAX_MESSAGE_BYTES, decode, encode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use snow::TransportState;

/// Noise's limit on one message, ciphertext included.
pub(crate) const NOISE_MAX: usize = 65535;
const TAG: usize = 16;
/// Plaintext per chunk: one flag byte, the rest data.
const CHUNK: usize = NOISE_MAX - TAG - 1;
const MORE: u8 = 1;
const LAST: u8 = 0;

pub(crate) fn write_frame(mut writer: impl Write, bytes: &[u8]) -> io::Result<()> {
    let len = u32::try_from(bytes.len())
        .ok()
        .filter(|&n| n as usize <= NOISE_MAX)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "frame too large"))?;
    let mut frame = Vec::with_capacity(4 + bytes.len());
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(bytes);
    writer.write_all(&frame)
}

pub(crate) fn read_frame(mut reader: impl Read) -> io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    reader.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len) as usize;
    if len > NOISE_MAX {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut bytes = vec![0u8; len];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn invalid(why: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.to_string())
}

/// An authenticated, encrypted connection. `send` and `recv` may run on different
/// threads at once: the Noise state is locked only while encrypting or decrypting.
pub(crate) struct Channel {
    stream: TcpStream,
    noise: Mutex<TransportState>,
    /// Keeps one message's chunks together.
    sending: Mutex<()>,
}

impl Channel {
    pub(crate) fn new(stream: TcpStream, noise: TransportState) -> Self {
        Self {
            stream,
            noise: Mutex::new(noise),
            sending: Mutex::new(()),
        }
    }

    pub(crate) fn send<T: Serialize>(&self, message: &T) -> io::Result<()> {
        let bytes = encode(message);
        let _whole = self
            .sending
            .lock()
            .map_err(|_| invalid("channel poisoned"))?;
        let chunks: Vec<&[u8]> = bytes.chunks(CHUNK).collect();
        let last = chunks.len().saturating_sub(1);
        for (i, chunk) in chunks.into_iter().enumerate() {
            let mut plain = Vec::with_capacity(chunk.len() + 1);
            plain.push(if i == last { LAST } else { MORE });
            plain.extend_from_slice(chunk);
            let mut sealed = vec![0u8; plain.len() + TAG];
            let n = self
                .noise
                .lock()
                .map_err(|_| invalid("channel poisoned"))?
                .write_message(&plain, &mut sealed)
                .map_err(invalid)?;
            write_frame(&self.stream, &sealed[..n])?;
        }
        Ok(())
    }

    /// The next message. Anything that fails to decrypt, arrives out of order or grows
    /// past the size limit ends the channel with an error.
    pub(crate) fn recv<T: DeserializeOwned>(&self) -> io::Result<T> {
        let mut message = Vec::new();
        loop {
            let sealed = read_frame(&self.stream)?;
            let mut plain = vec![0u8; sealed.len()];
            let n = self
                .noise
                .lock()
                .map_err(|_| invalid("channel poisoned"))?
                .read_message(&sealed, &mut plain)
                .map_err(invalid)?;
            let (flag, data) = plain[..n]
                .split_first()
                .ok_or_else(|| invalid("empty chunk"))?;
            if message.len() + data.len() > MAX_MESSAGE_BYTES {
                return Err(invalid("message too large"));
            }
            message.extend_from_slice(data);
            match *flag {
                LAST => return decode(&message).map_err(invalid),
                MORE => {}
                _ => return Err(invalid("bad chunk flag")),
            }
        }
    }

    /// Ends the connection; a `recv` blocked on another thread returns an error.
    pub(crate) fn close(&self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}
