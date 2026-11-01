//! Clipboard sync protocol between `cometdesk-gui` and `cometdesk-agent` (docs/clipboard-bridge.md).
//!
//! The two ends talk through the relay on the Comet, which forwards bytes blindly and resets
//! both sides whenever either one reconnects, so every connection starts a fresh session:
//!
//! - **Framing**: `u32` big-endian length + one Noise message (at most 65535 bytes).
//! - **Noise** `NNpsk0_25519_ChaChaPoly_BLAKE2s`, keyed by the 32-byte pairing secret. The
//!   GUI is the initiator and sends the first handshake message as soon as it connects; the
//!   agent answers. A wrong secret fails the handshake.
//! - **Messages** (postcard, inside the transport): `Hello` both ways, then `Policy` from
//!   the GUI, clipboard items as `Start` + `Chunk`s, `Ping`/`Pong`, and text insertion:
//!   `Insert` (GUI) → `InsertReply` (agent) → `InsertPasted` (GUI, when the agent asked for
//!   a paste keystroke).
//!
//! Sans-IO: feed received bytes to [`Endpoint::receive`], write [`Endpoint::take_outgoing`].

mod content;
mod endpoint;
mod error;
mod limits;
mod pairing;
mod protocol;
mod receive;
mod send;
#[cfg(test)]
mod tests;
mod wire;

pub use content::{Content, ContentKey, Dedup, encode_png};
pub use endpoint::Endpoint;
pub use error::{Error, Result};
pub use limits::{
    FILE_CHUNK, MAX_CLIP_FILES, MAX_CLIP_FILES_BYTES, MAX_FILE, MAX_INSERT, MAX_ITEM,
    MAX_NOTIFY_BODY, MAX_NOTIFY_TITLE, MAX_UPDATE, PROTOCOL, UPDATE_PROTOCOL,
};
pub use pairing::Secret;
pub use protocol::{
    Battery, DirEntry, DirTarget, Display, Focus, Hello, Info, InsertReply, Listing, Load,
    PasteKey, PastePref, Policy, PowerAction, Purpose, Role, SessionState, Usage, Volume,
};
pub use receive::{FileReceiver, create_restricted_dir_all, default_staging_dir};
pub use send::{FileSender, Pump, expand};
pub use wire::Event;
