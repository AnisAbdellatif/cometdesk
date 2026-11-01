//! Error and `Result` types for `cometdesk-clip` sessions and framing.

use crate::limits::{MAX_ITEM, PROTOCOL};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("pairing secret mismatch or corrupted handshake")]
    Handshake,
    #[error("decryption failed")]
    Crypto,
    #[error("bad frame: {0}")]
    Frame(&'static str),
    #[error("bad message: {0}")]
    Message(String),
    #[error("peer speaks protocol {0}, we speak {PROTOCOL}")]
    Version(u16),
    #[error("item too large ({0} bytes, max {MAX_ITEM})")]
    TooLarge(usize),
    #[error("not ready: handshake not finished")]
    NotReady,
    #[error("image: {0}")]
    Image(String),
}

pub type Result<T> = std::result::Result<T, Error>;
