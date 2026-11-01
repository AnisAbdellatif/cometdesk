//! Noise `NNpsk0` handshake setup and transition into transport mode for `Endpoint`.
//! Sends the local `Hello` message once the Noise handshake finishes.

use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::limits::{MAX_NOISE_MSG, NOISE_PARAMS, PROLOGUE};
use crate::pairing::Secret;
use crate::protocol::{Hello, Role};
use crate::wire::{Msg, Noise, push_frame};

use super::Endpoint;

impl Endpoint {
    /// A client queues its first handshake message right away.
    pub fn new(role: Role, secret: &Secret, hello: Hello) -> Result<Endpoint> {
        let builder = snow::Builder::new(NOISE_PARAMS.parse().map_err(|_| Error::Handshake)?)
            .psk(0, secret.as_bytes())
            .and_then(|b| b.prologue(PROLOGUE))
            .map_err(|_| Error::Handshake)?;
        let hs = match role {
            Role::Client => builder.build_initiator(),
            Role::Agent => builder.build_responder(),
        }
        .map_err(|_| Error::Handshake)?;
        let mut ep = Endpoint {
            role,
            noise: Noise::Handshake(Box::new(hs)),
            hello,
            peer: None,
            inbuf: Vec::new(),
            out: Vec::new(),
            scratch: Vec::new(),
            incoming: None,
            files: HashMap::new(),
            next_id: 1,
            limited: false,
        };
        if role == Role::Client {
            ep.write_handshake()?;
        }
        Ok(ep)
    }

    pub(super) fn write_handshake(&mut self) -> Result<()> {
        let Noise::Handshake(hs) = &mut self.noise else {
            return Ok(());
        };
        let mut buf = vec![0u8; MAX_NOISE_MSG];
        let n = hs
            .write_message(&[], &mut buf)
            .map_err(|_| Error::Handshake)?;
        push_frame(&mut self.out, &buf[..n]);
        self.maybe_finish_handshake()
    }

    pub(super) fn maybe_finish_handshake(&mut self) -> Result<()> {
        let finished = matches!(&self.noise, Noise::Handshake(hs) if hs.is_handshake_finished());
        if !finished {
            return Ok(());
        }
        let Noise::Handshake(hs) = std::mem::replace(&mut self.noise, Noise::Failed) else {
            unreachable!()
        };
        let transport = hs.into_transport_mode().map_err(|_| Error::Handshake)?;
        self.noise = Noise::Transport(Box::new(transport));
        let hello = self.hello.clone();
        self.send(&Msg::Hello(hello))
    }
}
