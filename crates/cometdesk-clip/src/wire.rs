//! Wire messages (`Msg`), caller-visible `Event`, Noise state, and frame helpers.
//! Keep `Msg` variant order append-only so `UPDATE_PROTOCOL` sessions stay compatible.

use serde::{Deserialize, Serialize};

use crate::content::Content;
use crate::protocol::{
    DirTarget, Hello, Info, InsertReply, Listing, PastePref, Policy, PowerAction, Purpose,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Kind {
    Text,
    Png,
    /// A screenshot answering a capture request; `id` is the request's.
    Screenshot,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum Msg {
    Hello(Hello),
    Policy(Policy),
    Start {
        id: u32,
        kind: Kind,
        len: u32,
        sha256: [u8; 32],
    },
    Chunk {
        id: u32,
        data: Vec<u8>,
    },
    Ping,
    Pong,
    Insert {
        id: u32,
        text: String,
        paste: PastePref,
    },
    InsertReply {
        id: u32,
        reply: InsertReply,
    },
    InsertPasted {
        id: u32,
    },
    Info(Info),
    Capture {
        id: u32,
        /// Index into `Info::displays`; `None`: all of them.
        display: Option<u32>,
    },
    CaptureFailed {
        id: u32,
        reason: String,
    },
    Power {
        id: u32,
        action: PowerAction,
    },
    /// Switch the active keyboard layout; `layout` is one of `Info::layouts`.
    SetLayout {
        id: u32,
        layout: String,
    },
    /// Answers `Power` and `SetLayout`. `Err` carries why the OS refused; `Ok` means it was
    /// asked, not that it finished.
    Ack {
        id: u32,
        result: std::result::Result<(), String>,
    },
    /// A message for the user, from the agent (a script on the target, or the agent itself).
    Notify {
        title: String,
        body: String,
    },
    /// A file for the peer to save, streamed as `FileData`s up to the `FileEnd`.
    FileOffer {
        id: u32,
        name: String,
        size: u64,
        purpose: Purpose,
    },
    /// Every file of a clipboard batch has arrived: put them on the clipboard, in this order.
    FilesReady {
        ids: Vec<u32>,
    },
    FileData {
        id: u32,
        data: Vec<u8>,
    },
    FileEnd {
        id: u32,
        sha256: [u8; 32],
    },
    /// Either side gives up on a transfer.
    FileCancel {
        id: u32,
    },
    /// The agent's verdict on a file: where it was saved, or why it was not.
    FileResult {
        id: u32,
        result: std::result::Result<String, String>,
    },
    /// Set the master volume and/or mute; answered with `Ack`.
    SetVolume {
        id: u32,
        level: Option<u8>,
        muted: Option<bool>,
    },
    /// Change a display's resolution and/or make it the primary one; answered with `Ack`.
    SetDisplay {
        id: u32,
        /// Index into `Info::displays`.
        display: u32,
        mode: Option<(u32, u32)>,
        primary: bool,
    },
    ListDir {
        id: u32,
        target: DirTarget,
    },
    DirList {
        id: u32,
        result: std::result::Result<Listing, String>,
    },
    /// Send a file of the target to the GUI (a `FileOffer` with [`Purpose::Save`] follows);
    /// `Ack` says whether it will.
    Fetch {
        id: u32,
        dir: String,
        name: String,
    },
}

/// Something the caller must act on.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The peer said hello: the session is ready for clipboard items.
    Ready(Hello),
    /// Clipboard content from the peer.
    Clip(Content),
    /// New policy from the GUI (agent only).
    Policy(Policy),
    /// Insert `text` where the target's focus is (agent only).
    Insert {
        id: u32,
        text: String,
        paste: PastePref,
    },
    /// The agent's answer to an insert (GUI only).
    InsertReply {
        id: u32,
        reply: InsertReply,
    },
    /// The GUI sent the paste keystroke (agent only).
    InsertPasted {
        id: u32,
    },
    /// The agent's view of the target changed (GUI only).
    Info(Info),
    /// Take a screenshot and send it back with [`Endpoint::send_screenshot`] (agent only).
    Capture {
        id: u32,
        display: Option<u32>,
    },
    /// A screenshot as PNG bytes, answering `Capture` `id` (GUI only).
    Screenshot {
        id: u32,
        png: Vec<u8>,
    },
    /// The agent could not capture (GUI only).
    CaptureFailed {
        id: u32,
        reason: String,
    },
    /// Lock, sleep, log off, restart or shut down the target (agent only).
    Power {
        id: u32,
        action: PowerAction,
    },
    /// Switch the target's keyboard layout (agent only).
    SetLayout {
        id: u32,
        layout: String,
    },
    /// A notification to show the user (GUI only), already cut to length.
    Notify {
        title: String,
        body: String,
    },
    /// The peer offers a file; save it under `name`, a bare file name the sender chose and the
    /// receiver must not trust.
    FileOffer {
        id: u32,
        name: String,
        size: u64,
        purpose: Purpose,
    },
    /// A clipboard batch is complete; the receiver collects the files it staged.
    FilesReady {
        ids: Vec<u32>,
    },
    FileData {
        id: u32,
        data: Vec<u8>,
    },
    /// All of the file was sent; `intact` says whether it hashes to what the sender announced.
    FileEnd {
        id: u32,
        intact: bool,
    },
    FileCancel {
        id: u32,
    },
    /// Where the peer saved a file, or why it could not.
    FileResult {
        id: u32,
        result: std::result::Result<String, String>,
    },
    /// The agent's answer to a power, layout, volume, display or fetch request (GUI only).
    Ack {
        id: u32,
        result: std::result::Result<(), String>,
    },
    SetVolume {
        id: u32,
        level: Option<u8>,
        muted: Option<bool>,
    },
    SetDisplay {
        id: u32,
        display: u32,
        mode: Option<(u32, u32)>,
        primary: bool,
    },
    ListDir {
        id: u32,
        target: DirTarget,
    },
    DirList {
        id: u32,
        result: std::result::Result<Listing, String>,
    },
    Fetch {
        id: u32,
        dir: String,
        name: String,
    },
    /// The peer runs another protocol version that can still be updated: the session only
    /// carries a [`Purpose::Update`] transfer (see [`UPDATE_PROTOCOL`]).
    Mismatch(Hello),
}

pub(crate) enum Noise {
    Handshake(Box<snow::HandshakeState>),
    Transport(Box<snow::TransportState>),
    /// Only while switching from handshake to transport, or after that failed.
    Failed,
}

pub(crate) struct Incoming {
    pub(crate) id: u32,
    pub(crate) kind: Kind,
    pub(crate) len: usize,
    pub(crate) sha256: [u8; 32],
    pub(crate) data: Vec<u8>,
}

/// At most `max` characters, control characters replaced by spaces.
pub(crate) fn cut(text: &str, max: usize) -> String {
    text.chars()
        .take(max)
        .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
        .collect()
}

pub(crate) fn push_frame(out: &mut Vec<u8>, payload: &[u8]) {
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
}
