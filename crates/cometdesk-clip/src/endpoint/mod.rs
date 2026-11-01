//! Sans-IO session endpoint pairing Noise framing with `cometdesk-clip` messages.
//! Handshake lives in `handshake`, outbound queueing in `send_loop`, inbound in `recv_loop`.

mod handshake;
mod recv_loop;
mod send_loop;

use std::collections::HashMap;

use sha2::Sha256;

use crate::protocol::{Hello, Role};
use crate::wire::{Incoming, Noise};

/// One end of a session. Create a new one for every connection.
pub struct Endpoint {
    role: Role,
    noise: Noise,
    hello: Hello,
    peer: Option<Hello>,
    inbuf: Vec<u8>,
    out: Vec<u8>,
    /// Reused for the plaintext/ciphertext of a single Noise message, to avoid a 64 KiB
    /// allocation per message.
    scratch: Vec<u8>,
    incoming: Option<Incoming>,
    /// Running hashes of the files in flight, in either direction.
    files: HashMap<u32, Sha256>,
    next_id: u32,
    /// The peer speaks another protocol: only update transfers are served.
    limited: bool,
}

impl Endpoint {
    /// The peer's hello, once received.
    pub fn peer(&self) -> Option<&Hello> {
        self.peer.as_ref()
    }

    pub fn is_ready(&self) -> bool {
        self.peer.is_some()
    }

    /// Bytes to write to the connection (drains the queue).
    pub fn take_outgoing(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }

    pub fn has_outgoing(&self) -> bool {
        !self.out.is_empty()
    }

    /// Bytes queued for the wire: a sender waits for this to drain before queueing more.
    pub fn outgoing_len(&self) -> usize {
        self.out.len()
    }

    fn take_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        id
    }
}
