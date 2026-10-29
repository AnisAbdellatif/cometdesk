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
mod pairing;
mod receive;
mod send;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use content::{Content, ContentKey, Dedup, encode_png};
pub use pairing::Secret;
pub use receive::FileReceiver;
pub use send::{FileSender, Pump};

/// Bumped on incompatible protocol changes; both ends refuse a different major.
pub const PROTOCOL: u16 = 7;
/// Longest text for one [`Endpoint::send_insert`] (one Noise message).
pub const MAX_INSERT: usize = 60 * 1024;
/// Largest clipboard item (encoded: UTF-8 text or PNG).
pub const MAX_ITEM: usize = 16 * 1024 * 1024;
/// Largest file for [`Endpoint::send_file_offer`]; files are streamed, not held in memory.
pub const MAX_FILE: u64 = 64 * 1024 * 1024 * 1024;
/// Longest piece of file data for one [`Endpoint::send_file_data`].
pub const FILE_CHUNK: usize = 60 * 1024;
/// Copied files travel on their own, so each one and the whole batch stay small enough to
/// send without asking; larger ones go through "Send file".
pub const MAX_CLIP_FILES_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_CLIP_FILES: usize = 64;
/// Notification text the agent forwards is cut to this many characters.
pub const MAX_NOTIFY_TITLE: usize = 100;
pub const MAX_NOTIFY_BODY: usize = 500;

/// What the receiver does with a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Purpose {
    /// Keep it in the Downloads folder.
    Save,
    /// Stage it, and put it on the clipboard with the rest of its batch on `FilesReady`.
    Clipboard,
}

const NOISE_PARAMS: &str = "Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s";
const PROLOGUE: &[u8] = b"cometdesk-clip v1";
const MAX_NOISE_MSG: usize = 65535;
const TAG_LEN: usize = 16;
const CHUNK: usize = 60 * 1024;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// cometdesk-gui: Noise initiator.
    Client,
    /// cometdesk-agent on the target: Noise responder.
    Agent,
}

/// What each end tells the other once the session is up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub protocol: u16,
    /// `macos`, `linux`, `windows`.
    pub os: String,
    /// Host name of the machine, for display.
    pub name: String,
    /// Software version (`CARGO_PKG_VERSION`).
    pub version: String,
}

impl Hello {
    pub fn local(version: &str, name: String) -> Hello {
        Hello {
            protocol: PROTOCOL,
            os: std::env::consts::OS.to_owned(),
            name,
            version: version.to_owned(),
        }
    }
}

/// What the agent knows about the target, sent after `Hello` and whenever it changes.
/// Every field is best effort: `None` or empty when the OS does not say.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Info {
    /// Active keyboard layout, tagged by source: `xkb:de+nodeadkeys`, `win:0407`,
    /// `mac:com.apple.keylayout.German`. The GUI maps it to a device keymap.
    pub keyboard: Option<String>,
    /// Every layout installed on the target, same tagging; the active one is not marked.
    pub layouts: Vec<String>,
    /// OS name and version for display (`Ubuntu 24.04 LTS`, `macOS 15.1`).
    pub os_version: Option<String>,
    pub session: SessionState,
    /// The monitors of the target, in desktop coordinates.
    pub displays: Vec<Display>,
    /// An input method editor (CJK, …) is on: key-by-key typing may be composed into
    /// something else.
    pub ime: Option<bool>,
    /// What has the keyboard focus. Only the application, never window titles.
    pub focus: Option<Focus>,
    /// Account the agent runs as.
    pub user: Option<String>,
    /// Seconds since the target booted. Not compared when deciding whether to resend.
    pub uptime_secs: Option<u64>,
    pub battery: Option<Battery>,
    /// Load, memory and system-drive use. Moves constantly: not compared when deciding
    /// whether to resend (the agent resends it on a timer instead).
    pub load: Option<Load>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    #[default]
    Unknown,
    Active,
    /// Lock screen or secure desktop: typed text goes to a password field, not an app.
    Locked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Display {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Percent: 100, 125, 150, 200.
    pub scale: u32,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Focus {
    /// Executable or window class (`notepad.exe`, `gnome-terminal-server`).
    pub app: String,
    pub terminal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
}

/// Resource use of the target.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Load {
    /// CPU busy over the last poll, 0-100; `None` until two samples exist.
    pub cpu_percent: Option<u8>,
    pub memory: Option<Usage>,
    /// The system drive (`C:`, `/`).
    pub disk: Option<Usage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub used_mb: u64,
    pub total_mb: u64,
}

impl Info {
    /// Equality for deciding whether to resend: uptime and load always move.
    pub fn same_as(&self, other: &Info) -> bool {
        let mut a = self.clone();
        a.uptime_secs = other.uptime_secs;
        a.load = other.load;
        a == *other
    }
}

/// What the GUI can ask the agent to do to the target's session or power state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerAction {
    Lock,
    Sleep,
    Logoff,
    Restart,
    Shutdown,
}

impl PowerAction {
    pub fn label(self) -> &'static str {
        match self {
            PowerAction::Lock => "Lock",
            PowerAction::Sleep => "Sleep",
            PowerAction::Logoff => "Log off",
            PowerAction::Restart => "Restart",
            PowerAction::Shutdown => "Shut down",
        }
    }

    /// Loses the user's unsaved work or the agent connection for good.
    pub fn destructive(self) -> bool {
        matches!(
            self,
            PowerAction::Logoff | PowerAction::Restart | PowerAction::Shutdown
        )
    }
}

/// Sent by the GUI; the agent must not send its clipboard unless allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub agent_may_send: bool,
}

/// Which keystroke pastes on the target, as the GUI prefers it for Linux targets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PastePref {
    /// The agent decides (terminals get Ctrl+Shift+V where it can tell).
    #[default]
    Auto,
    CtrlV,
    CtrlShiftV,
}

/// A paste keystroke the GUI sends over the KVM's keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasteKey {
    CtrlV,
    CtrlShiftV,
    CmdV,
}

/// The agent's answer to an insert request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsertReply {
    /// Inserted directly (Windows: one `SendInput` with Unicode characters).
    Inserted,
    /// The text is on the target's clipboard: send this keystroke, then
    /// [`Endpoint::send_insert_pasted`] so the agent restores the previous clipboard.
    PasteNow(PasteKey),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Kind {
    Text,
    Png,
    /// A screenshot answering a capture request; `id` is the request's.
    Screenshot,
}

#[derive(Debug, Serialize, Deserialize)]
enum Msg {
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
    /// The agent's answer to a power or layout request (GUI only).
    Ack {
        id: u32,
        result: std::result::Result<(), String>,
    },
}

enum Noise {
    Handshake(Box<snow::HandshakeState>),
    Transport(Box<snow::TransportState>),
    /// Only while switching from handshake to transport, or after that failed.
    Failed,
}

struct Incoming {
    id: u32,
    kind: Kind,
    len: usize,
    sha256: [u8; 32],
    data: Vec<u8>,
}

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
}

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
        };
        if role == Role::Client {
            ep.write_handshake()?;
        }
        Ok(ep)
    }

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

    /// Feeds received bytes; returns the resulting events. Any error ends the session.
    pub fn receive(&mut self, bytes: &[u8]) -> Result<Vec<Event>> {
        self.inbuf.extend_from_slice(bytes);
        let mut events = Vec::new();
        loop {
            if self.inbuf.len() < 4 {
                break;
            }
            let len = u32::from_be_bytes(self.inbuf[..4].try_into().unwrap()) as usize;
            if len == 0 || len > MAX_NOISE_MSG {
                return Err(Error::Frame("length out of range"));
            }
            if self.inbuf.len() < 4 + len {
                break;
            }
            let frame: Vec<u8> = self.inbuf.drain(..4 + len).skip(4).collect();
            self.on_frame(&frame, &mut events)?;
        }
        Ok(events)
    }

    pub fn set_policy(&mut self, policy: Policy) -> Result<()> {
        self.send(&Msg::Policy(policy))
    }

    /// Asks the agent to insert `text` at the target's focus. Returns the request id.
    pub fn send_insert(&mut self, text: &str, paste: PastePref) -> Result<u32> {
        if !self.is_ready() {
            return Err(Error::NotReady);
        }
        if text.len() > MAX_INSERT {
            return Err(Error::TooLarge(text.len()));
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.send(&Msg::Insert {
            id,
            text: text.to_owned(),
            paste,
        })?;
        Ok(id)
    }

    pub fn send_insert_reply(&mut self, id: u32, reply: InsertReply) -> Result<()> {
        self.send(&Msg::InsertReply { id, reply })
    }

    pub fn send_insert_pasted(&mut self, id: u32) -> Result<()> {
        self.send(&Msg::InsertPasted { id })
    }

    pub fn send_info(&mut self, info: &Info) -> Result<()> {
        self.send(&Msg::Info(info.clone()))
    }

    pub fn send_capture(&mut self, display: Option<u32>) -> Result<u32> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.send(&Msg::Capture { id, display })?;
        Ok(id)
    }

    pub fn send_power(&mut self, action: PowerAction) -> Result<u32> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.send(&Msg::Power { id, action })?;
        Ok(id)
    }

    pub fn send_set_layout(&mut self, layout: &str) -> Result<u32> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.send(&Msg::SetLayout {
            id,
            layout: layout.to_owned(),
        })?;
        Ok(id)
    }

    /// Starts sending a file: follow with [`Endpoint::send_file_data`] pieces and
    /// [`Endpoint::send_file_end`].
    pub fn send_file_offer(&mut self, name: &str, size: u64, purpose: Purpose) -> Result<u32> {
        if size > MAX_FILE {
            return Err(Error::Message("file too large".into()));
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.send(&Msg::FileOffer {
            id,
            name: name.to_owned(),
            size,
            purpose,
        })?;
        self.files.insert(id, Sha256::new());
        Ok(id)
    }

    pub fn send_file_data(&mut self, id: u32, data: &[u8]) -> Result<()> {
        if data.len() > FILE_CHUNK {
            return Err(Error::TooLarge(data.len()));
        }
        let hash = self
            .files
            .get_mut(&id)
            .ok_or_else(|| Error::Message(format!("no file {id} is being sent")))?;
        hash.update(data);
        self.send(&Msg::FileData {
            id,
            data: data.to_vec(),
        })
    }

    pub fn send_file_end(&mut self, id: u32) -> Result<()> {
        let hash = self
            .files
            .remove(&id)
            .ok_or_else(|| Error::Message(format!("no file {id} is being sent")))?;
        self.send(&Msg::FileEnd {
            id,
            sha256: hash.finalize().into(),
        })
    }

    /// Texts longer than [`MAX_NOTIFY_TITLE`] / [`MAX_NOTIFY_BODY`] characters are cut.
    pub fn send_notify(&mut self, title: &str, body: &str) -> Result<()> {
        self.send(&Msg::Notify {
            title: cut(title, MAX_NOTIFY_TITLE),
            body: cut(body, MAX_NOTIFY_BODY),
        })
    }

    pub fn send_files_ready(&mut self, ids: &[u32]) -> Result<()> {
        self.send(&Msg::FilesReady { ids: ids.to_vec() })
    }

    pub fn send_file_cancel(&mut self, id: u32) -> Result<()> {
        self.files.remove(&id);
        self.send(&Msg::FileCancel { id })
    }

    pub fn send_file_result(
        &mut self,
        id: u32,
        result: std::result::Result<String, String>,
    ) -> Result<()> {
        self.send(&Msg::FileResult { id, result })
    }

    /// Bytes queued for the wire: a sender waits for this to drain before queueing more.
    pub fn outgoing_len(&self) -> usize {
        self.out.len()
    }

    pub fn send_ack(&mut self, id: u32, result: std::result::Result<(), String>) -> Result<()> {
        self.send(&Msg::Ack { id, result })
    }

    pub fn send_capture_failed(&mut self, id: u32, reason: String) -> Result<()> {
        self.send(&Msg::CaptureFailed { id, reason })
    }

    /// Answers a capture request with PNG bytes (see [`encode_png`]).
    pub fn send_screenshot(&mut self, id: u32, png: &[u8]) -> Result<()> {
        self.send_item(id, Kind::Screenshot, png)
    }

    pub fn ping(&mut self) -> Result<()> {
        self.send(&Msg::Ping)
    }

    /// Queues a clipboard item for the peer.
    pub fn send_clip(&mut self, content: &Content) -> Result<()> {
        if !self.is_ready() {
            return Err(Error::NotReady);
        }
        let (kind, wire) = match content {
            Content::Text(t) => (Kind::Text, t.as_bytes().to_vec()),
            Content::Image { .. } => (Kind::Png, content.to_png()?),
            Content::Files(_) => {
                return Err(Error::Message("files go through the file transfer".into()));
            }
        };
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.send_item(id, kind, &wire)
    }

    fn send_item(&mut self, id: u32, kind: Kind, wire: &[u8]) -> Result<()> {
        if wire.len() > MAX_ITEM {
            return Err(Error::TooLarge(wire.len()));
        }
        self.send(&Msg::Start {
            id,
            kind,
            len: wire.len() as u32,
            sha256: Sha256::digest(wire).into(),
        })?;
        for data in wire.chunks(CHUNK) {
            self.send(&Msg::Chunk {
                id,
                data: data.to_vec(),
            })?;
        }
        Ok(())
    }

    // ----- internals

    fn write_handshake(&mut self) -> Result<()> {
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

    fn maybe_finish_handshake(&mut self) -> Result<()> {
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

    fn on_frame(&mut self, frame: &[u8], events: &mut Vec<Event>) -> Result<()> {
        // The plaintext never exceeds the ciphertext. Own the buffer (not a `self` borrow) so
        // the arms can still call `&mut self` methods; restore it for the next message.
        let mut plain = std::mem::take(&mut self.scratch);
        plain.resize(frame.len(), 0);
        let result = match &mut self.noise {
            Noise::Handshake(hs) => hs
                .read_message(frame, &mut plain)
                .map_err(|_| Error::Handshake)
                .and_then(|_| {
                    if self.role == Role::Agent {
                        self.write_handshake()
                    } else {
                        self.maybe_finish_handshake()
                    }
                }),
            Noise::Transport(t) => t
                .read_message(frame, &mut plain)
                .map_err(|_| Error::Crypto)
                .and_then(|n| {
                    postcard::from_bytes(&plain[..n]).map_err(|e| Error::Message(e.to_string()))
                })
                .and_then(|msg| self.on_msg(msg, events)),
            Noise::Failed => Err(Error::Handshake),
        };
        plain.clear();
        self.scratch = plain;
        result
    }

    fn on_msg(&mut self, msg: Msg, events: &mut Vec<Event>) -> Result<()> {
        match msg {
            Msg::Hello(h) => {
                if h.protocol != PROTOCOL {
                    return Err(Error::Version(h.protocol));
                }
                self.peer = Some(h.clone());
                events.push(Event::Ready(h));
            }
            Msg::Info(i) => {
                if self.role == Role::Client {
                    events.push(Event::Info(i));
                }
            }
            Msg::Capture { id, display } if self.role == Role::Agent => {
                events.push(Event::Capture { id, display });
            }
            Msg::CaptureFailed { id, reason } if self.role == Role::Client => {
                events.push(Event::CaptureFailed { id, reason });
            }
            Msg::Capture { .. } | Msg::CaptureFailed { .. } => {
                return Err(Error::Message("capture message for the wrong role".into()));
            }
            Msg::Notify { title, body } if self.role == Role::Client => {
                events.push(Event::Notify {
                    title: cut(&title, MAX_NOTIFY_TITLE),
                    body: cut(&body, MAX_NOTIFY_BODY),
                });
            }
            Msg::Notify { .. } => {
                return Err(Error::Message("notification for the wrong role".into()));
            }
            Msg::FileOffer {
                id,
                name,
                size,
                purpose,
            } => {
                if size > MAX_FILE {
                    return Err(Error::Message("file too large".into()));
                }
                self.files.insert(id, Sha256::new());
                events.push(Event::FileOffer {
                    id,
                    name,
                    size,
                    purpose,
                });
            }
            Msg::FilesReady { ids } => {
                if ids.len() > MAX_CLIP_FILES {
                    return Err(Error::Message("too many files".into()));
                }
                events.push(Event::FilesReady { ids });
            }
            Msg::FileData { id, data } => {
                // Unknown ids are a cancelled or refused file whose data was already in
                // flight: drop it quietly.
                if let Some(hash) = self.files.get_mut(&id) {
                    hash.update(&data);
                    events.push(Event::FileData { id, data });
                }
            }
            Msg::FileEnd { id, sha256 } => {
                if let Some(hash) = self.files.remove(&id) {
                    let intact = <[u8; 32]>::from(hash.finalize()) == sha256;
                    events.push(Event::FileEnd { id, intact });
                }
            }
            Msg::FileCancel { id } => {
                self.files.remove(&id);
                events.push(Event::FileCancel { id });
            }
            Msg::FileResult { id, result } => {
                events.push(Event::FileResult { id, result });
            }
            Msg::Power { id, action } if self.role == Role::Agent => {
                events.push(Event::Power { id, action });
            }
            Msg::SetLayout { id, layout } if self.role == Role::Agent => {
                events.push(Event::SetLayout { id, layout });
            }
            Msg::Ack { id, result } if self.role == Role::Client => {
                events.push(Event::Ack { id, result });
            }
            Msg::Power { .. } | Msg::SetLayout { .. } | Msg::Ack { .. } => {
                return Err(Error::Message("request for the wrong role".into()));
            }
            Msg::Policy(p) => {
                if self.role == Role::Agent {
                    events.push(Event::Policy(p));
                }
            }
            Msg::Start {
                id,
                kind,
                len,
                sha256,
            } => {
                let len = len as usize;
                if len > MAX_ITEM {
                    return Err(Error::TooLarge(len));
                }
                self.incoming = Some(Incoming {
                    id,
                    kind,
                    len,
                    sha256,
                    data: Vec::with_capacity(len),
                });
                if len == 0 {
                    self.finish_item(events)?;
                }
            }
            Msg::Chunk { id, data } => {
                let Some(inc) = self.incoming.as_mut().filter(|i| i.id == id) else {
                    return Err(Error::Message(format!("chunk for unknown item {id}")));
                };
                if inc.data.len() + data.len() > inc.len {
                    return Err(Error::Message("item longer than announced".into()));
                }
                inc.data.extend_from_slice(&data);
                if inc.data.len() == inc.len {
                    self.finish_item(events)?;
                }
            }
            Msg::Ping => self.send(&Msg::Pong)?,
            Msg::Pong => {}
            Msg::Insert { id, text, paste } if self.role == Role::Agent => {
                events.push(Event::Insert { id, text, paste });
            }
            Msg::InsertReply { id, reply } if self.role == Role::Client => {
                events.push(Event::InsertReply { id, reply });
            }
            Msg::InsertPasted { id } if self.role == Role::Agent => {
                events.push(Event::InsertPasted { id });
            }
            Msg::Insert { .. } | Msg::InsertReply { .. } | Msg::InsertPasted { .. } => {
                return Err(Error::Message("insert message for the wrong role".into()));
            }
        }
        Ok(())
    }

    fn finish_item(&mut self, events: &mut Vec<Event>) -> Result<()> {
        let inc = self.incoming.take().expect("item in progress");
        if <[u8; 32]>::from(Sha256::digest(&inc.data)) != inc.sha256 {
            return Err(Error::Message("item checksum mismatch".into()));
        }
        if inc.kind == Kind::Screenshot {
            events.push(Event::Screenshot {
                id: inc.id,
                png: inc.data,
            });
            return Ok(());
        }
        let content = match inc.kind {
            Kind::Text => Content::Text(
                String::from_utf8(inc.data)
                    .map_err(|_| Error::Message("text is not UTF-8".into()))?,
            ),
            Kind::Png => Content::from_png(&inc.data)?,
            Kind::Screenshot => unreachable!(),
        };
        events.push(Event::Clip(content));
        Ok(())
    }

    fn send(&mut self, msg: &Msg) -> Result<()> {
        let Noise::Transport(t) = &mut self.noise else {
            return Err(Error::NotReady);
        };
        let plain = postcard::to_allocvec(msg).map_err(|e| Error::Message(e.to_string()))?;
        debug_assert!(plain.len() + TAG_LEN <= MAX_NOISE_MSG);
        self.scratch.resize(plain.len() + TAG_LEN, 0);
        let n = t
            .write_message(&plain, &mut self.scratch)
            .map_err(|_| Error::Crypto)?;
        push_frame(&mut self.out, &self.scratch[..n]);
        self.scratch.clear();
        Ok(())
    }
}

/// At most `max` characters, control characters replaced by spaces.
fn cut(text: &str, max: usize) -> String {
    text.chars()
        .take(max)
        .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
        .collect()
}

fn push_frame(out: &mut Vec<u8>, payload: &[u8]) {
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(secret_a: &Secret, secret_b: &Secret) -> (Endpoint, Endpoint) {
        let c = Endpoint::new(Role::Client, secret_a, Hello::local("1", "mac".into())).unwrap();
        let a = Endpoint::new(Role::Agent, secret_b, Hello::local("1", "laptop".into())).unwrap();
        (c, a)
    }

    /// Moves bytes both ways in small pieces until quiet; returns (client events, agent events).
    fn pump(c: &mut Endpoint, a: &mut Endpoint) -> Result<(Vec<Event>, Vec<Event>)> {
        let (mut ce, mut ae) = (Vec::new(), Vec::new());
        while c.has_outgoing() || a.has_outgoing() {
            for piece in c.take_outgoing().chunks(7) {
                ae.extend(a.receive(piece)?);
            }
            for piece in a.take_outgoing().chunks(7) {
                ce.extend(c.receive(piece)?);
            }
        }
        Ok((ce, ae))
    }

    #[test]
    fn info_goes_from_agent_to_gui_only() {
        let s = Secret::generate();
        let (mut c, mut a) = pair(&s, &s);
        pump(&mut c, &mut a).unwrap();
        let info = Info {
            keyboard: Some("xkb:de".into()),
            os_version: Some("Ubuntu 24.04".into()),
            session: SessionState::Locked,
            uptime_secs: Some(5),
            ..Info::default()
        };
        a.send_info(&info).unwrap();
        c.send_info(&info).unwrap();
        let (ce, ae) = pump(&mut c, &mut a).unwrap();
        assert_eq!(ce, vec![Event::Info(info)]);
        assert!(ae.is_empty());
    }

    #[test]
    fn power_and_layout_round_trip_and_wrong_role() {
        let s = Secret::generate();
        let (mut c, mut a) = pair(&s, &s);
        pump(&mut c, &mut a).unwrap();
        let id = c.send_power(PowerAction::Restart).unwrap();
        let (_, ae) = pump(&mut c, &mut a).unwrap();
        assert_eq!(
            ae,
            vec![Event::Power {
                id,
                action: PowerAction::Restart
            }]
        );
        a.send_ack(id, Err("denied".into())).unwrap();
        let (ce, _) = pump(&mut c, &mut a).unwrap();
        assert_eq!(
            ce,
            vec![Event::Ack {
                id,
                result: Err("denied".into())
            }]
        );
        let id = c.send_set_layout("xkb:de").unwrap();
        let (_, ae) = pump(&mut c, &mut a).unwrap();
        assert_eq!(
            ae,
            vec![Event::SetLayout {
                id,
                layout: "xkb:de".into()
            }]
        );
        a.send_power(PowerAction::Lock).unwrap();
        assert!(pump(&mut c, &mut a).is_err());
    }

    #[test]
    fn file_round_trip_detects_corruption() {
        let s = Secret::generate();
        let (mut c, mut a) = pair(&s, &s);
        pump(&mut c, &mut a).unwrap();
        let id = c.send_file_offer("a.bin", 6, Purpose::Save).unwrap();
        c.send_file_data(id, b"abc").unwrap();
        c.send_file_data(id, b"def").unwrap();
        c.send_file_end(id).unwrap();
        let (_, ae) = pump(&mut c, &mut a).unwrap();
        assert_eq!(
            ae,
            vec![
                Event::FileOffer {
                    id,
                    name: "a.bin".into(),
                    size: 6,
                    purpose: Purpose::Save
                },
                Event::FileData {
                    id,
                    data: b"abc".to_vec()
                },
                Event::FileData {
                    id,
                    data: b"def".to_vec()
                },
                Event::FileEnd { id, intact: true },
            ]
        );
        a.send_file_result(id, Ok("/tmp/a.bin".into())).unwrap();
        let (ce, _) = pump(&mut c, &mut a).unwrap();
        assert_eq!(
            ce,
            vec![Event::FileResult {
                id,
                result: Ok("/tmp/a.bin".into())
            }]
        );
        // Data for a cancelled file is not sent, and one that is already in flight is ignored.
        let id = c.send_file_offer("b", 1, Purpose::Clipboard).unwrap();
        c.send_file_cancel(id).unwrap();
        c.send_file_data(id, b"x").unwrap_err();
        let (_, ae) = pump(&mut c, &mut a).unwrap();
        assert_eq!(ae.len(), 2);
        // Both directions work: the agent can send files too.
        let id = a.send_file_offer("c", 0, Purpose::Clipboard).unwrap();
        a.send_file_end(id).unwrap();
        a.send_files_ready(&[id]).unwrap();
        let (ce, _) = pump(&mut c, &mut a).unwrap();
        assert_eq!(ce.last(), Some(&Event::FilesReady { ids: vec![id] }));
    }

    #[test]
    fn clipboard_file_streams_into_staging() {
        let dir = std::env::temp_dir().join(format!("cometdesk-clip-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("data.bin");
        let bytes: Vec<u8> = (0..200_000u32).map(|i| (i * 7) as u8).collect();
        std::fs::write(&src, &bytes).unwrap();

        let s = Secret::generate();
        let (mut c, mut a) = pair(&s, &s);
        pump(&mut c, &mut a).unwrap();
        let mut rx = FileReceiver::new(dir.join("staging"));
        let mut tx = FileSender::start(&mut c, &src, "data.bin", Purpose::Clipboard).unwrap();
        let mut done = false;
        while !done {
            done = matches!(tx.pump(&mut c).unwrap(), Pump::Finished);
            let (_, ae) = pump(&mut c, &mut a).unwrap();
            for ev in ae {
                match ev {
                    Event::FileOffer {
                        id,
                        name,
                        size,
                        purpose,
                    } => rx.offer(id, &name, size, purpose).unwrap(),
                    Event::FileData { id, data } => rx.data(id, &data).unwrap(),
                    Event::FileEnd { id, intact } => {
                        rx.end(id, intact).unwrap();
                    }
                    other => panic!("unexpected {other:?}"),
                }
            }
        }
        let staged = rx.take_staged(&[tx.id]).unwrap();
        assert_eq!(staged[0].file_name().unwrap(), "data.bin");
        assert_eq!(std::fs::read(&staged[0]).unwrap(), bytes);
        assert!(rx.take_staged(&[tx.id]).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn notifications_go_to_the_gui_and_are_cut() {
        let s = Secret::generate();
        let (mut c, mut a) = pair(&s, &s);
        pump(&mut c, &mut a).unwrap();
        a.send_notify(&"t".repeat(300), "done\u{7}").unwrap();
        let (ce, _) = pump(&mut c, &mut a).unwrap();
        assert_eq!(
            ce,
            vec![Event::Notify {
                title: "t".repeat(MAX_NOTIFY_TITLE),
                body: "done ".into()
            }]
        );
        c.send_notify("x", "y").unwrap();
        assert!(pump(&mut c, &mut a).is_err());
    }

    #[test]
    fn load_does_not_trigger_a_resend() {
        let a = Info::default();
        let b = Info {
            load: Some(Load {
                cpu_percent: Some(9),
                ..Load::default()
            }),
            ..Info::default()
        };
        assert!(a.same_as(&b));
    }

    #[test]
    fn screenshot_round_trip_and_failure() {
        let s = Secret::generate();
        let (mut c, mut a) = pair(&s, &s);
        pump(&mut c, &mut a).unwrap();
        let id = c.send_capture(Some(1)).unwrap();
        let (_, ae) = pump(&mut c, &mut a).unwrap();
        assert_eq!(
            ae,
            vec![Event::Capture {
                id,
                display: Some(1)
            }]
        );
        let png: Vec<u8> = (0..150_000u32).map(|i| i as u8).collect();
        a.send_screenshot(id, &png).unwrap();
        let (ce, _) = pump(&mut c, &mut a).unwrap();
        assert_eq!(ce, vec![Event::Screenshot { id, png }]);
        a.send_capture_failed(id, "locked".into()).unwrap();
        let (ce, _) = pump(&mut c, &mut a).unwrap();
        assert_eq!(
            ce,
            vec![Event::CaptureFailed {
                id,
                reason: "locked".into()
            }]
        );
    }

    #[test]
    fn handshake_hello_and_items_both_ways() {
        let s = Secret::generate();
        let (mut c, mut a) = pair(&s, &s);
        let (ce, ae) = pump(&mut c, &mut a).unwrap();
        assert!(matches!(&ce[..], [Event::Ready(h)] if h.name == "laptop"));
        assert!(matches!(&ae[..], [Event::Ready(h)] if h.name == "mac"));

        c.set_policy(Policy {
            agent_may_send: false,
        })
        .unwrap();
        c.send_clip(&Content::Text("héllo\nworld".into())).unwrap();
        let img = Content::Image {
            width: 3,
            height: 2,
            rgba: (0..24).collect(),
        };
        a.send_clip(&img).unwrap();
        let (ce, ae) = pump(&mut c, &mut a).unwrap();
        assert_eq!(
            ae,
            vec![
                Event::Policy(Policy {
                    agent_may_send: false
                }),
                Event::Clip(Content::Text("héllo\nworld".into()))
            ]
        );
        assert_eq!(ce, vec![Event::Clip(img)]);
    }

    #[test]
    fn insert_round_trip() {
        let s = Secret::generate();
        let (mut c, mut a) = pair(&s, &s);
        pump(&mut c, &mut a).unwrap();
        let id = c.send_insert("a|b@c\n€", PastePref::CtrlShiftV).unwrap();
        let (_, ae) = pump(&mut c, &mut a).unwrap();
        assert_eq!(
            ae,
            vec![Event::Insert {
                id,
                text: "a|b@c\n€".into(),
                paste: PastePref::CtrlShiftV
            }]
        );
        a.send_insert_reply(id, InsertReply::PasteNow(PasteKey::CtrlV))
            .unwrap();
        let (ce, _) = pump(&mut c, &mut a).unwrap();
        assert_eq!(
            ce,
            vec![Event::InsertReply {
                id,
                reply: InsertReply::PasteNow(PasteKey::CtrlV)
            }]
        );
        c.send_insert_pasted(id).unwrap();
        let (_, ae) = pump(&mut c, &mut a).unwrap();
        assert_eq!(ae, vec![Event::InsertPasted { id }]);
        assert!(matches!(
            c.send_insert(&"x".repeat(MAX_INSERT + 1), PastePref::Auto),
            Err(Error::TooLarge(_))
        ));
        // The agent cannot ask the GUI to insert.
        a.send(&Msg::Insert {
            id: 9,
            text: "x".into(),
            paste: PastePref::Auto,
        })
        .unwrap();
        assert!(pump(&mut c, &mut a).is_err());
    }

    #[test]
    fn large_item_is_chunked() {
        let s = Secret::generate();
        let (mut c, mut a) = pair(&s, &s);
        pump(&mut c, &mut a).unwrap();
        let text = "x".repeat(1_000_000);
        c.send_clip(&Content::Text(text.clone())).unwrap();
        let (_, ae) = pump(&mut c, &mut a).unwrap();
        assert_eq!(ae, vec![Event::Clip(Content::Text(text))]);
    }

    #[test]
    fn wrong_secret_fails_the_handshake() {
        let (mut c, mut a) = pair(&Secret::generate(), &Secret::generate());
        assert!(matches!(pump(&mut c, &mut a), Err(Error::Handshake)));
    }

    #[test]
    fn ping_is_answered() {
        let s = Secret::generate();
        let (mut c, mut a) = pair(&s, &s);
        pump(&mut c, &mut a).unwrap();
        c.ping().unwrap();
        let (ce, ae) = pump(&mut c, &mut a).unwrap();
        assert!(ce.is_empty() && ae.is_empty());
    }

    #[test]
    fn items_before_handshake_are_refused() {
        let s = Secret::generate();
        let (mut c, _a) = pair(&s, &s);
        assert!(matches!(
            c.send_clip(&Content::Text("x".into())),
            Err(Error::NotReady)
        ));
    }

    #[test]
    fn garbage_frame_is_an_error() {
        let s = Secret::generate();
        let (_c, mut a) = pair(&s, &s);
        assert!(a.receive(&[0, 0, 0, 0]).is_err());
        let (_c, mut a) = pair(&s, &s);
        assert!(a.receive(&[0, 0, 0, 3, 1, 2, 3]).is_err());
    }
}
