//! Clipboard sync protocol between `glkvm-gui` and `glkvm-agent` (docs/clipboard-bridge.md).
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

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use content::{Content, ContentKey, Dedup, encode_png};
pub use pairing::Secret;

/// Bumped on incompatible protocol changes; both ends refuse a different major.
pub const PROTOCOL: u16 = 4;
/// Longest text for one [`Endpoint::send_insert`] (one Noise message).
pub const MAX_INSERT: usize = 60 * 1024;
/// Largest clipboard item (encoded: UTF-8 text or PNG).
pub const MAX_ITEM: usize = 16 * 1024 * 1024;

const NOISE_PARAMS: &str = "Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s";
const PROLOGUE: &[u8] = b"glkvm-clip v1";
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
    /// glkvm-gui: Noise initiator.
    Client,
    /// glkvm-agent on the target: Noise responder.
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

impl Info {
    /// Equality for deciding whether to resend: uptime always moves.
    pub fn same_as(&self, other: &Info) -> bool {
        let mut a = self.clone();
        a.uptime_secs = other.uptime_secs;
        a == *other
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
    InsertReply { id: u32, reply: InsertReply },
    /// The GUI sent the paste keystroke (agent only).
    InsertPasted { id: u32 },
    /// The agent's view of the target changed (GUI only).
    Info(Info),
    /// Take a screenshot and send it back with [`Endpoint::send_screenshot`] (agent only).
    Capture { id: u32, display: Option<u32> },
    /// A screenshot as PNG bytes, answering `Capture` `id` (GUI only).
    Screenshot { id: u32, png: Vec<u8> },
    /// The agent could not capture (GUI only).
    CaptureFailed { id: u32, reason: String },
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
    incoming: Option<Incoming>,
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
            incoming: None,
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
        let mut plain = vec![0u8; MAX_NOISE_MSG];
        match &mut self.noise {
            Noise::Handshake(hs) => {
                hs.read_message(frame, &mut plain)
                    .map_err(|_| Error::Handshake)?;
                if self.role == Role::Agent {
                    self.write_handshake()?;
                } else {
                    self.maybe_finish_handshake()?;
                }
                Ok(())
            }
            Noise::Transport(t) => {
                let n = t
                    .read_message(frame, &mut plain)
                    .map_err(|_| Error::Crypto)?;
                let msg: Msg =
                    postcard::from_bytes(&plain[..n]).map_err(|e| Error::Message(e.to_string()))?;
                self.on_msg(msg, events)
            }
            Noise::Failed => Err(Error::Handshake),
        }
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
        let mut buf = vec![0u8; plain.len() + TAG_LEN];
        let n = t
            .write_message(&plain, &mut buf)
            .map_err(|_| Error::Crypto)?;
        push_frame(&mut self.out, &buf[..n]);
        Ok(())
    }
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
