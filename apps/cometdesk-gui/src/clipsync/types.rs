//! Shared request, progress, batch, and notification types for the clipboard bridge.
//!
//! Read `bridge.rs` for the public channel API and `run.rs` for request dispatch.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

use cometdesk::events::HidSender;
use cometdesk_clip::{DirTarget, FileSender, Hello, Listing, PasteKey, PastePref, PowerAction};
use tokio::sync::oneshot;
use tokio::time::Instant;

/// Outcome of an insert: a short description, or why it failed.
pub type InsertResult = Result<String, String>;

/// PNG bytes of the target's screen, or why there are none.
pub type CaptureResult = Result<Vec<u8>, String>;

/// Whether the agent's OS accepted a request, or why not.
pub type AckResult = Result<(), String>;

/// Something the agent does to the target and answers with a plain yes or no.
pub enum Request {
    Power(PowerAction),
    Layout(String),
    Volume {
        level: Option<u8>,
        muted: Option<bool>,
    },
    /// Resolution and/or primary status of display `index` (of `Info::displays`).
    Display {
        index: u32,
        mode: Option<(u32, u32)>,
        primary: bool,
    },
    /// Download `name` from folder `dir` of the target into Downloads.
    Fetch {
        dir: String,
        name: String,
    },
}

/// A folder of the target, or why it cannot be listed.
pub type ListResult = Result<Listing, String>;

pub(super) struct DirRequest {
    pub(super) target: DirTarget,
    pub(super) reply: oneshot::Sender<ListResult>,
}

/// What the bridge remembers across connections, for noticing reboots and for not offering
/// the same agent update twice.
#[derive(Default)]
pub(super) struct Memory {
    /// The target's uptime in its last report.
    pub(super) uptime: Option<u64>,
    /// The agent (version, protocol) an update was already sent to.
    pub(super) update_tried: Option<(String, u16)>,
}

/// The target's uptime went down since the last report: it restarted (a sleep does not
/// reset it).
pub(super) fn restarted(last: Option<u64>, uptime: u64) -> bool {
    last.is_some_and(|prev| uptime + 5 < prev)
}

/// Whether `peer` runs an older agent than this app ships.
pub(super) fn agent_is_older(peer: &Hello) -> bool {
    let version = |v: &str| -> Vec<u32> { v.split('.').filter_map(|p| p.parse().ok()).collect() };
    match peer.protocol.cmp(&cometdesk_clip::PROTOCOL) {
        std::cmp::Ordering::Equal => version(&peer.version) < version(env!("CARGO_PKG_VERSION")),
        o => o.is_lt(),
    }
}

/// Shared view of a running file transfer, for the UI.
pub struct TransferProgress {
    pub name: String,
    pub total: u64,
    pub sent: AtomicU64,
    pub cancel: AtomicBool,
}

/// A file on its way to the target. `result` holds where the agent saved it.
pub struct FileSend {
    pub progress: Arc<TransferProgress>,
    pub result: oneshot::Receiver<Result<String, String>>,
}

pub(super) struct FileRequest {
    pub(super) path: PathBuf,
    pub(super) progress: Arc<TransferProgress>,
    pub(super) reply: oneshot::Sender<Result<String, String>>,
    /// Part of a clipboard batch (its number): the files are staged on the target and put on
    /// its clipboard together.
    pub(super) batch: Option<u32>,
    /// A new agent program, to replace the one running on the target.
    pub(super) update: bool,
}

/// The transfer being streamed, or waiting for the agent's verdict.
pub(super) struct ActiveFile {
    pub(super) sender: FileSender,
    pub(super) req: FileRequest,
    pub(super) finished_at: Option<Instant>,
}

impl ActiveFile {
    pub(super) fn fail(self, why: impl Into<String>) {
        let _ = self.req.reply.send(Err(why.into()));
    }
}

/// A clipboard batch in flight: its files' transfer ids as they complete.
pub(super) struct Batch {
    pub(super) ids: Vec<u32>,
    pub(super) remaining: usize,
}

/// Something the bridge wants the user to hear about.
pub enum Note {
    /// From the app itself, such as copied files it could not send.
    Info(String),
    /// From the target: a script's `cometdesk-agent notify`, or the agent's own alert.
    Target { title: String, body: String },
}

pub(super) struct AckRequest {
    pub(super) request: Request,
    pub(super) reply: oneshot::Sender<AckResult>,
}

pub(super) struct CaptureRequest {
    pub(super) display: Option<u32>,
    pub(super) reply: oneshot::Sender<CaptureResult>,
}

pub(super) struct InsertRequest {
    pub(super) text: String,
    pub(super) paste: PastePref,
    /// The console's current keyboard path (it is replaced when the console reconnects).
    pub(super) hid: HidSender,
    pub(super) reply: oneshot::Sender<InsertResult>,
}

/// Keys for the paste keystroke the agent asks for.
pub(super) fn paste_keys(key: PasteKey) -> &'static [&'static str] {
    match key {
        PasteKey::CtrlV => &["ControlLeft", "KeyV"],
        PasteKey::CtrlShiftV => &["ControlLeft", "ShiftLeft", "KeyV"],
        PasteKey::CmdV => &["MetaLeft", "KeyV"],
    }
}
