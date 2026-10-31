//! Clipboard bridge client: syncs this Mac's clipboard with `cometdesk-agent` on the target,
//! through the relay on the Comet (`/cometdesk-bridge/ws`, docs/clipboard-bridge.md).
//!
//! One task per console. It reconnects on its own; every connection is a fresh end-to-end
//! encrypted session (the relay resets both sides when either reconnects). Only changes made
//! while connected are sent, never what was on the clipboard before, and content marked
//! concealed or transient by password managers is never sent.
//!
//! It also inserts text on the target through the agent ([`ClipBridge::insert`]): in one
//! step instead of a stream of key presses that a window switch could misdirect. When the
//! agent asks for a paste keystroke, this task sends it over the KVM's keyboard.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cometdesk::Session;
use cometdesk::api::bridge::BRIDGE_WS;
use cometdesk::client::WsStream;
use cometdesk::events::HidSender;
use cometdesk_clip::{
    Content, Dedup, DirTarget, Endpoint, Error, Event, FileReceiver, FileSender, Hello, Info,
    InsertReply, Listing, PasteKey, PastePref, Policy, PowerAction, Pump, Purpose, Role, Secret,
};
use futures_util::{SinkExt, StreamExt};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior, interval};
use tokio_tungstenite::tungstenite::Message;

use crate::store::ClipDirection;

const POLL: Duration = Duration::from_millis(300);
const PING: Duration = Duration::from_secs(15);
/// The agent answers every ping, so a session silent this long is dead (sleep, network change).
const SILENCE_TIMEOUT: Duration = Duration::from_secs(45);
const STATUS_EVERY: Duration = Duration::from_secs(5);
/// Agent connected per the relay, but no answer to our handshake for this long: assume a
/// dead connection and reset the session.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(15);
const RETRY_SLOW: Duration = Duration::from_secs(30);
/// While waiting to reconnect, how often to look for an agent already waiting on the relay.
const AGENT_CHECK: Duration = Duration::from_secs(2);
const INSERT_TIMEOUT: Duration = Duration::from_secs(5);
/// A 4K screenshot is several MB through the relay.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(20);

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

struct DirRequest {
    target: DirTarget,
    reply: oneshot::Sender<ListResult>,
}

/// What the bridge remembers across connections, for noticing reboots and for not offering
/// the same agent update twice.
#[derive(Default)]
struct Memory {
    /// The target's uptime in its last report.
    uptime: Option<u64>,
    /// The agent (version, protocol) an update was already sent to.
    update_tried: Option<(String, u16)>,
}

/// The target's uptime went down since the last report: it restarted (a sleep does not
/// reset it).
fn restarted(last: Option<u64>, uptime: u64) -> bool {
    last.is_some_and(|prev| uptime + 5 < prev)
}

/// Queues the agent program this app ships for `peer`, if `peer` is older and no update was
/// sent to that version yet. Whether anything was queued.
fn queue_update(
    peer: &Hello,
    memory: &mut Memory,
    queue: &mut VecDeque<FileRequest>,
    notes: &mpsc::UnboundedSender<Note>,
) -> bool {
    let tried = (peer.version.clone(), peer.protocol);
    if !agent_is_older(peer) || memory.update_tried.as_ref() == Some(&tried) {
        return false;
    }
    let Some(path) = crate::setup::agent_binary(&peer.os, &peer.arch) else {
        return false;
    };
    let Ok(meta) = std::fs::metadata(&path) else {
        return false;
    };
    memory.update_tried = Some(tried);
    let (reply, _) = oneshot::channel();
    queue.push_front(FileRequest {
        progress: Arc::new(TransferProgress {
            name: "cometdesk-agent".into(),
            total: meta.len(),
            sent: AtomicU64::new(0),
            cancel: AtomicBool::new(false),
        }),
        path,
        reply,
        batch: None,
        update: true,
    });
    let _ = notes.send(Note::Info(format!(
        "Updating the agent on the target ({} to {})",
        peer.version,
        env!("CARGO_PKG_VERSION")
    )));
    true
}

/// Whether `peer` runs an older agent than this app ships.
fn agent_is_older(peer: &Hello) -> bool {
    let version = |v: &str| -> Vec<u32> { v.split('.').filter_map(|p| p.parse().ok()).collect() };
    match peer.protocol.cmp(&cometdesk_clip::PROTOCOL) {
        std::cmp::Ordering::Equal => version(&peer.version) < version(env!("CARGO_PKG_VERSION")),
        o => o.is_lt(),
    }
}

/// How long the agent gets to verify, sync and rename a file after its last byte.
const FILE_FINISH_TIMEOUT: Duration = Duration::from_secs(60);

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

struct FileRequest {
    path: PathBuf,
    progress: Arc<TransferProgress>,
    reply: oneshot::Sender<Result<String, String>>,
    /// Part of a clipboard batch (its number): the files are staged on the target and put on
    /// its clipboard together.
    batch: Option<u32>,
    /// A new agent program, to replace the one running on the target.
    update: bool,
}

/// The transfer being streamed, or waiting for the agent's verdict.
struct ActiveFile {
    sender: FileSender,
    req: FileRequest,
    finished_at: Option<Instant>,
}

impl ActiveFile {
    fn fail(self, why: impl Into<String>) {
        let _ = self.req.reply.send(Err(why.into()));
    }
}

/// A clipboard batch in flight: its files' transfer ids as they complete.
struct Batch {
    ids: Vec<u32>,
    remaining: usize,
}

/// Something the bridge wants the user to hear about.
pub enum Note {
    /// From the app itself, such as copied files it could not send.
    Info(String),
    /// From the target: a script's `cometdesk-agent notify`, or the agent's own alert.
    Target { title: String, body: String },
}

struct AckRequest {
    request: Request,
    reply: oneshot::Sender<AckResult>,
}

struct CaptureRequest {
    display: Option<u32>,
    reply: oneshot::Sender<CaptureResult>,
}

struct InsertRequest {
    text: String,
    paste: PastePref,
    /// The console's current keyboard path (it is replaced when the console reconnects).
    hid: HidSender,
    reply: oneshot::Sender<InsertResult>,
}

/// Keys for the paste keystroke the agent asks for.
fn paste_keys(key: PasteKey) -> &'static [&'static str] {
    match key {
        PasteKey::CtrlV => &["ControlLeft", "KeyV"],
        PasteKey::CtrlShiftV => &["ControlLeft", "ShiftLeft", "KeyV"],
        PasteKey::CmdV => &["MetaLeft", "KeyV"],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum BridgeState {
    Connecting,
    /// The relay is not installed on this device (`bridge/install.sh`).
    NotInstalled,
    /// Relay reached; no agent connected on the target.
    WaitingForAgent,
    /// The agent answered with a different pairing code.
    Mismatch,
    /// The agent speaks another protocol version: set up the target again.
    AgentOutdated(u16),
    /// The agent is older than this app and is being sent a new program.
    Updating,
    /// The agent's hello, and what it last told us about the target.
    Connected(Hello, Info),
    Error(String),
}

impl BridgeState {
    pub fn label(&self) -> String {
        match self {
            BridgeState::Connecting => "connecting…".into(),
            BridgeState::NotInstalled => "bridge not installed on this device".into(),
            BridgeState::WaitingForAgent => "waiting for the agent on the target".into(),
            BridgeState::Mismatch => "agent has a different pairing code".into(),
            BridgeState::AgentOutdated(_) => {
                "the agent on the target is outdated: set up the target again".into()
            }
            BridgeState::Updating => "updating the agent on the target…".into(),
            BridgeState::Connected(h, _) => format!("connected to {} ({})", h.name, h.os),
            BridgeState::Error(e) => format!("error: {e}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub direction: ClipDirection,
    pub paused: bool,
}

impl Settings {
    fn sends(self) -> bool {
        !self.paused && self.direction.sends()
    }
    fn receives(self) -> bool {
        !self.paused && self.direction.receives()
    }
    fn policy(self) -> Policy {
        Policy {
            agent_may_send: self.receives(),
        }
    }
}

pub struct ClipBridge {
    state: watch::Receiver<BridgeState>,
    settings: watch::Sender<Settings>,
    inserts: mpsc::UnboundedSender<InsertRequest>,
    captures: mpsc::UnboundedSender<CaptureRequest>,
    acks: mpsc::UnboundedSender<AckRequest>,
    dirs: mpsc::UnboundedSender<DirRequest>,
    files: mpsc::UnboundedSender<FileRequest>,
    notes: Mutex<mpsc::UnboundedReceiver<Note>>,
    task: JoinHandle<()>,
}

impl ClipBridge {
    /// `wake` is called whenever the state changes, to refresh the UI.
    pub fn spawn(
        rt: &Handle,
        session: Session,
        secret: Secret,
        settings: Settings,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> ClipBridge {
        let (state_tx, state) = watch::channel(BridgeState::Connecting);
        let (settings, settings_rx) = watch::channel(settings);
        let (inserts, inserts_rx) = mpsc::unbounded_channel();
        let (captures, captures_rx) = mpsc::unbounded_channel();
        let (acks, acks_rx) = mpsc::unbounded_channel();
        let (dirs, dirs_rx) = mpsc::unbounded_channel();
        let (files, files_rx) = mpsc::unbounded_channel();
        let (notes_tx, notes) = mpsc::unbounded_channel();
        let task = rt.spawn(supervise(
            session,
            secret,
            settings_rx,
            inserts_rx,
            captures_rx,
            acks_rx,
            dirs_rx,
            files_rx,
            notes_tx,
            state_tx,
            wake,
        ));
        ClipBridge {
            state,
            settings,
            inserts,
            captures,
            acks,
            dirs,
            files,
            notes: Mutex::new(notes),
            task,
        }
    }

    pub fn state(&self) -> BridgeState {
        self.state.borrow().clone()
    }

    pub fn settings(&self) -> Settings {
        *self.settings.borrow()
    }

    pub fn set_settings(&self, s: Settings) {
        self.settings.send_replace(s);
    }

    /// Whether text can be inserted through the agent right now.
    pub fn can_insert(&self) -> bool {
        matches!(*self.state.borrow(), BridgeState::Connected(..))
    }

    /// Asks the agent for a pixel-exact screenshot (PNG). `display` indexes the agent's
    /// reported displays; `None` takes all of them.
    pub fn capture(&self, display: Option<u32>) -> oneshot::Receiver<CaptureResult> {
        let (reply, rx) = oneshot::channel();
        let _ = self.captures.send(CaptureRequest { display, reply });
        rx
    }

    /// Streams a file to the target's Downloads folder through the agent. Files queue and go
    /// one at a time.
    pub fn send_file(&self, path: PathBuf) -> Result<FileSend, String> {
        let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
        if !meta.is_file() {
            return Err("not a file".into());
        }
        if meta.len() > cometdesk_clip::MAX_FILE {
            return Err("the file is too large".into());
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("the file name is not valid UTF-8")?
            .to_owned();
        let progress = Arc::new(TransferProgress {
            name,
            total: meta.len(),
            sent: AtomicU64::new(0),
            cancel: AtomicBool::new(false),
        });
        let (reply, result) = oneshot::channel();
        let req = FileRequest {
            path,
            progress: progress.clone(),
            reply,
            batch: None,
            update: false,
        };
        self.files
            .send(req)
            .map_err(|_| "the clipboard bridge is stopped".to_string())?;
        Ok(FileSend { progress, result })
    }

    /// Things the bridge wants the user to hear about, such as copied files it could not send.
    pub fn take_notes(&self) -> Vec<Note> {
        let mut rx = self.notes.lock().unwrap();
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    }

    /// Asks the agent to lock, sleep, log off, restart or shut down the target, or to
    /// switch its keyboard layout.
    pub fn request(&self, request: Request) -> oneshot::Receiver<AckResult> {
        let (reply, rx) = oneshot::channel();
        let _ = self.acks.send(AckRequest { request, reply });
        rx
    }

    /// Lists a folder of the target's file system.
    pub fn list_dir(&self, target: DirTarget) -> oneshot::Receiver<ListResult> {
        let (reply, rx) = oneshot::channel();
        let _ = self.dirs.send(DirRequest { target, reply });
        rx
    }

    /// Inserts `text` at the target's focus through the agent. The receiver gets the
    /// outcome (an error if the connection drops first).
    pub fn insert(
        &self,
        text: String,
        paste: PastePref,
        hid: HidSender,
    ) -> oneshot::Receiver<InsertResult> {
        let (reply, rx) = oneshot::channel();
        let _ = self.inserts.send(InsertRequest {
            text,
            paste,
            hid,
            reply,
        });
        rx
    }
}

impl Drop for ClipBridge {
    fn drop(&mut self) {
        self.task.abort();
    }
}

enum End {
    Mismatch,
    NotInstalled,
    AgentOutdated(u16),
    /// Reason, and whether a session had been established.
    Lost(String, bool),
}

#[allow(clippy::too_many_arguments)]
async fn supervise(
    session: Session,
    secret: Secret,
    mut settings: watch::Receiver<Settings>,
    mut inserts: mpsc::UnboundedReceiver<InsertRequest>,
    mut captures: mpsc::UnboundedReceiver<CaptureRequest>,
    mut acks: mpsc::UnboundedReceiver<AckRequest>,
    mut dirs: mpsc::UnboundedReceiver<DirRequest>,
    mut files: mpsc::UnboundedReceiver<FileRequest>,
    notes: mpsc::UnboundedSender<Note>,
    state: watch::Sender<BridgeState>,
    wake: impl Fn() + Send + Sync + 'static,
) {
    let set = |s: BridgeState| {
        if state.send_if_modified(|cur| {
            let changed = *cur != s;
            *cur = s;
            changed
        }) {
            wake();
        }
    };
    let mut local = LocalClipboard::new().await;
    let mut memory = Memory::default();
    let mut backoff = BACKOFF_MIN;
    loop {
        set(BridgeState::Connecting);
        let end = match session.bridge_status().await {
            Ok(None) => End::NotInstalled,
            Err(e) => End::Lost(e.to_string(), false),
            Ok(Some(_)) => match session.ws_connect(BRIDGE_WS).await {
                Ok(ws) => {
                    run(
                        ws,
                        &session,
                        &secret,
                        &mut settings,
                        &mut inserts,
                        &mut captures,
                        &mut acks,
                        &mut dirs,
                        &mut files,
                        &notes,
                        &mut local,
                        &mut memory,
                        &set,
                        &wake,
                    )
                    .await
                }
                Err(e) => End::Lost(e.to_string(), false),
            },
        };
        let wait = match end {
            End::NotInstalled => {
                set(BridgeState::NotInstalled);
                RETRY_SLOW
            }
            End::Mismatch => {
                set(BridgeState::Mismatch);
                RETRY_SLOW
            }
            End::AgentOutdated(v) => {
                set(BridgeState::AgentOutdated(v));
                RETRY_SLOW
            }
            End::Lost(reason, was_up) => {
                tracing::info!(%reason, "clipboard bridge connection lost");
                set(BridgeState::Error(reason));
                if was_up {
                    backoff = BACKOFF_MIN;
                }
                let w = backoff;
                backoff = (backoff * 2).min(BACKOFF_MAX);
                w
            }
        };
        // Inserts asked for while disconnected fail now rather than after the wait.
        let deadline = tokio::time::Instant::now() + wait;
        // A freshly installed agent is waiting on the relay: don't make it wait out a long
        // backoff (up to RETRY_SLOW) that started before it existed.
        let mut agent_check = tokio::time::interval(AGENT_CHECK);
        agent_check.tick().await;
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => break,
                _ = agent_check.tick() => {
                    if let Ok(Some(st)) = session.bridge_status().await
                        && st.agent.connected
                        && !st.client.connected
                    {
                        break;
                    }
                }
                Some(req) = inserts.recv() => {
                    let _ = req.reply.send(Err("the agent is not connected".into()));
                }
                Some(req) = captures.recv() => {
                    let _ = req.reply.send(Err("the agent is not connected".into()));
                }
                Some(req) = acks.recv() => {
                    let _ = req.reply.send(Err("the agent is not connected".into()));
                }
                Some(req) = dirs.recv() => {
                    let _ = req.reply.send(Err("the agent is not connected".into()));
                }
                Some(req) = files.recv() => {
                    let _ = req.reply.send(Err("the agent is not connected".into()));
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run(
    ws: WsStream,
    session: &Session,
    secret: &Secret,
    settings: &mut watch::Receiver<Settings>,
    inserts: &mut mpsc::UnboundedReceiver<InsertRequest>,
    captures: &mut mpsc::UnboundedReceiver<CaptureRequest>,
    acks: &mut mpsc::UnboundedReceiver<AckRequest>,
    dirs: &mut mpsc::UnboundedReceiver<DirRequest>,
    files: &mut mpsc::UnboundedReceiver<FileRequest>,
    notes: &mpsc::UnboundedSender<Note>,
    local: &mut LocalClipboard,
    memory: &mut Memory,
    set: &impl Fn(BridgeState),
    wake: &(impl Fn() + Send + Sync),
) -> End {
    let (mut sink, mut stream) = ws.split();
    let hello = Hello::local(env!("CARGO_PKG_VERSION"), host_name());
    let mut ep = match Endpoint::new(Role::Client, secret, hello) {
        Ok(ep) => ep,
        Err(e) => return End::Lost(e.to_string(), false),
    };
    set(BridgeState::WaitingForAgent);
    let connected_at = Instant::now();
    let mut agent_paired_since: Option<Instant> = None;
    // Inserts waiting for the agent's reply: id -> (reply, keyboard, asked at).
    let mut pending: HashMap<u32, (oneshot::Sender<InsertResult>, HidSender, Instant)> =
        HashMap::new();
    let mut captured: HashMap<u32, (oneshot::Sender<CaptureResult>, Instant)> = HashMap::new();
    let mut queue: VecDeque<FileRequest> = VecDeque::new();
    let mut active: Option<ActiveFile> = None;
    let mut batches: HashMap<u32, Batch> = HashMap::new();
    let mut next_batch = 0u32;
    let mut receiver = FileReceiver::new(std::env::temp_dir().join("cometdesk-clip"));
    let mut last_wake = Instant::now();
    let mut acked: HashMap<u32, (oneshot::Sender<AckResult>, Instant)> = HashMap::new();
    let mut listed: HashMap<u32, (oneshot::Sender<ListResult>, Instant)> = HashMap::new();
    // Files the user asked to download that have not been offered or refused yet, and the
    // ones being received now.
    let mut fetches = 0u32;
    let mut fetch_acks: HashSet<u32> = HashSet::new();
    let mut downloading: HashSet<u32> = HashSet::new();
    let mut poll = interval(POLL);
    let mut ping = interval(PING);
    let mut status = interval(STATUS_EVERY);
    let mut last_rx = Instant::now();
    for t in [&mut poll, &mut ping, &mut status] {
        t.set_missed_tick_behavior(MissedTickBehavior::Delay);
    }
    loop {
        if ep.has_outgoing()
            && let Err(e) = sink.send(Message::Binary(ep.take_outgoing().into())).await
        {
            return End::Lost(e.to_string(), ep.is_ready());
        }
        let up = ep.is_ready();
        if up && active.is_none() {
            while let Some(req) = queue.pop_front() {
                match start_file(&mut ep, req) {
                    Ok(a) => {
                        active = Some(a);
                        break;
                    }
                    Err((req, e)) => {
                        drop_batch(req.batch, &mut batches, &mut queue);
                        let _ = req.reply.send(Err(e));
                    }
                }
            }
        }
        tokio::select! {
            // One piece at a time, so pings, the clipboard and replies keep flowing.
            _ = tokio::task::yield_now(), if active.as_ref().is_some_and(|a| a.finished_at.is_none()) => {
                if let Some(a) = active.take() {
                    active = stream_file(&mut ep, a, &mut batches, &mut queue);
                    // The progress bar needs redraws, but not one per piece.
                    if active.is_none() || last_wake.elapsed() > Duration::from_millis(100) {
                        last_wake = Instant::now();
                        wake();
                    }
                }
            }
            msg = stream.next() => {
                let data = match msg {
                    Some(Ok(Message::Binary(b))) => {
                        last_rx = Instant::now();
                        b
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        return End::Lost("relay closed the session".into(), up)
                    }
                    Some(Ok(_)) => continue,
                    Some(Err(e)) => return End::Lost(e.to_string(), up),
                };
                let events = match ep.receive(&data) {
                    Ok(ev) => ev,
                    Err(Error::Handshake) => return End::Mismatch,
                    Err(Error::Version(v)) => return End::AgentOutdated(v),
                    Err(e) => return End::Lost(e.to_string(), up),
                };
                for ev in events {
                    match ev {
                        Event::Ready(peer) => {
                            tracing::info!(agent = %peer.name, os = %peer.os, "clipboard bridge connected");
                            set(BridgeState::Connected(peer.clone(), Info::default()));
                            if let Err(e) = ep.set_policy(settings.borrow().policy()) {
                                return End::Lost(e.to_string(), true);
                            }
                            queue_update(&peer, memory, &mut queue, notes);
                        }
                        Event::Mismatch(peer) => {
                            tracing::info!(agent = %peer.name, protocol = peer.protocol, "agent speaks another protocol");
                            if peer.protocol > cometdesk_clip::PROTOCOL {
                                return End::Lost("the agent on the target is newer than this app".into(), true);
                            }
                            if !queue_update(&peer, memory, &mut queue, notes) {
                                return End::AgentOutdated(peer.protocol);
                            }
                            set(BridgeState::Updating);
                        }
                        Event::Clip(content) => {
                            if settings.borrow().receives() {
                                tracing::info!(?content, "clipboard from the target");
                                local.apply(content).await;
                            }
                        }
                        Event::InsertReply { id, reply } => {
                            let Some((tx, hid, _)) = pending.remove(&id) else { continue };
                            let outcome = match reply {
                                InsertReply::Inserted => Ok("Inserted on the target".into()),
                                InsertReply::PasteNow(key) => {
                                    for cmd in crate::actions::combo(paste_keys(key)) {
                                        hid.send(cmd);
                                    }
                                    if let Err(e) = ep.send_insert_pasted(id) {
                                        return End::Lost(e.to_string(), true);
                                    }
                                    Ok("Pasted on the target".into())
                                }
                                InsertReply::Failed(e) => Err(e),
                            };
                            let _ = tx.send(outcome);
                            wake();
                        }
                        Event::Info(info) => {
                            tracing::info!(?info, "target info");
                            if let Some(up) = info.uptime_secs {
                                if restarted(memory.uptime, up) {
                                    let _ = notes.send(Note::Target {
                                        title: "Target restarted".into(),
                                        body: "The target booted again and its agent is back.".into(),
                                    });
                                    wake();
                                }
                                memory.uptime = Some(up);
                            }
                            if let Some(peer) = ep.peer() {
                                set(BridgeState::Connected(peer.clone(), info));
                            }
                        }
                        Event::Screenshot { id, png } => {
                            if let Some((tx, _)) = captured.remove(&id) {
                                let _ = tx.send(Ok(png));
                                wake();
                            }
                        }
                        Event::CaptureFailed { id, reason } => {
                            if let Some((tx, _)) = captured.remove(&id) {
                                let _ = tx.send(Err(reason));
                                wake();
                            }
                        }
                        Event::Ack { id, result } => {
                            if fetch_acks.remove(&id) && result.is_err() {
                                fetches = fetches.saturating_sub(1);
                            }
                            if let Some((tx, _)) = acked.remove(&id) {
                                let _ = tx.send(result);
                                wake();
                            }
                        }
                        Event::DirList { id, result } => {
                            if let Some((tx, _)) = listed.remove(&id) {
                                let _ = tx.send(result);
                                wake();
                            }
                        }
                        Event::FileResult { id, result } => {
                            if active.as_ref().is_some_and(|a| a.sender.id == id)
                                && let Some(a) = active.take()
                            {
                                if a.req.update {
                                    let _ = notes.send(Note::Info(match &result {
                                        Ok(_) => "The agent on the target was updated and is restarting".into(),
                                        Err(e) => format!("Agent update failed: {e}"),
                                    }));
                                }
                                match &result {
                                    Ok(_) => {
                                        if let Some(b) = a.req.batch
                                            && let Some(batch) = batches.get_mut(&b)
                                        {
                                            batch.ids.push(id);
                                            batch.remaining -= 1;
                                            if batch.remaining == 0 {
                                                let batch = batches.remove(&b).expect("just seen");
                                                if let Err(e) = ep.send_files_ready(&batch.ids) {
                                                    return End::Lost(e.to_string(), true);
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        // The agent dropped it; free what it kept for this id.
                                        let _ = ep.send_file_cancel(id);
                                        if a.req.batch.is_some() {
                                            let _ = notes.send(Note::Info(format!(
                                                "Copied files not sent: {e}"
                                            )));
                                        }
                                        drop_batch(a.req.batch, &mut batches, &mut queue);
                                    }
                                }
                                let _ = a.req.reply.send(result);
                                wake();
                            }
                        }
                        // The agent sends files when something was copied on the target.
                        Event::FileOffer { id, name, size, purpose } => {
                            let refused = match purpose {
                                Purpose::Clipboard if settings.borrow().receives() => {
                                    receiver.offer(id, &name, size, purpose)
                                }
                                // Only what the user picked in the file browser.
                                Purpose::Save if fetches > 0 => {
                                    fetches -= 1;
                                    let r = receiver.offer(id, &name, size, purpose);
                                    if r.is_ok() {
                                        downloading.insert(id);
                                    }
                                    r
                                }
                                _ => Err("the GUI does not take files from the target".to_string()),
                            };
                            if let Err(e) = refused
                                && let Err(e) = ep.send_file_result(id, Err(e))
                            {
                                return End::Lost(e.to_string(), true);
                            }
                        }
                        Event::FileData { id, data } => {
                            if let Err(e) = receiver.data(id, &data) {
                                if downloading.remove(&id) {
                                    let _ = notes.send(Note::Info(format!("Download failed: {e}")));
                                    wake();
                                }
                                if let Err(e) = ep.send_file_result(id, Err(e)) {
                                    return End::Lost(e.to_string(), true);
                                }
                            }
                        }
                        Event::FileEnd { id, intact } => {
                            let result = receiver.end(id, intact);
                            if downloading.remove(&id) {
                                let _ = notes.send(Note::Info(match &result {
                                    Ok(path) => format!("Downloaded to {path}"),
                                    Err(e) => format!("Download failed: {e}"),
                                }));
                                wake();
                            }
                            if let Err(e) = ep.send_file_result(id, result) {
                                return End::Lost(e.to_string(), true);
                            }
                        }
                        Event::FileCancel { id } => {
                            receiver.cancel(id);
                            if downloading.remove(&id) {
                                let _ = notes.send(Note::Info("Download cancelled on the target".into()));
                                wake();
                            }
                        }
                        Event::FilesReady { ids } => {
                            if let Some(paths) = receiver.take_staged(&ids) {
                                tracing::info!(count = paths.len(), "copied files from the target");
                                local.apply(Content::Files(paths)).await;
                            }
                        }
                        Event::Notify { title, body } => {
                            let _ = notes.send(Note::Target { title, body });
                            wake();
                        }
                        Event::Policy(_)
                        | Event::Power { .. }
                        | Event::SetLayout { .. }
                        | Event::SetVolume { .. }
                        | Event::SetDisplay { .. }
                        | Event::ListDir { .. }
                        | Event::Fetch { .. }
                        | Event::Insert { .. }
                        | Event::InsertPasted { .. }
                        | Event::Capture { .. } => {}
                    }
                }
            }
            Some(req) = inserts.recv() => {
                if !up {
                    let _ = req.reply.send(Err("the agent is not connected".into()));
                    continue;
                }
                match ep.send_insert(&req.text, req.paste) {
                    Ok(id) => {
                        pending.insert(id, (req.reply, req.hid, Instant::now()));
                    }
                    Err(e) => {
                        let _ = req.reply.send(Err(e.to_string()));
                    }
                }
            }
            Some(req) = captures.recv() => {
                if !up {
                    let _ = req.reply.send(Err("the agent is not connected".into()));
                    continue;
                }
                match ep.send_capture(req.display) {
                    Ok(id) => {
                        captured.insert(id, (req.reply, Instant::now()));
                    }
                    Err(e) => {
                        let _ = req.reply.send(Err(e.to_string()));
                    }
                }
            }
            Some(req) = files.recv() => {
                if up {
                    queue.push_back(req);
                } else {
                    let _ = req.reply.send(Err("the agent is not connected".into()));
                }
            }
            Some(req) = dirs.recv() => {
                if !up {
                    let _ = req.reply.send(Err("the agent is not connected".into()));
                    continue;
                }
                match ep.send_list_dir(req.target) {
                    Ok(id) => {
                        listed.insert(id, (req.reply, Instant::now()));
                    }
                    Err(e) => {
                        let _ = req.reply.send(Err(e.to_string()));
                    }
                }
            }
            Some(req) = acks.recv() => {
                if !up {
                    let _ = req.reply.send(Err("the agent is not connected".into()));
                    continue;
                }
                let sent = match &req.request {
                    Request::Power(a) => ep.send_power(*a),
                    Request::Layout(l) => ep.send_set_layout(l),
                    Request::Volume { level, muted } => ep.send_set_volume(*level, *muted),
                    Request::Display { index, mode, primary } => {
                        ep.send_set_display(*index, *mode, *primary)
                    }
                    Request::Fetch { dir, name } => {
                        fetches += 1;
                        ep.send_fetch(dir, name)
                    }
                };
                match sent {
                    Ok(id) => {
                        if matches!(req.request, Request::Fetch { .. }) {
                            fetch_acks.insert(id);
                        }
                        acked.insert(id, (req.reply, Instant::now()));
                    }
                    Err(e) => {
                        let _ = req.reply.send(Err(e.to_string()));
                    }
                }
            }
            _ = poll.tick() => {
                if active
                    .as_ref()
                    .and_then(|a| a.finished_at)
                    .is_some_and(|t| t.elapsed() > FILE_FINISH_TIMEOUT)
                    && let Some(a) = active.take()
                {
                    a.fail("the agent did not confirm the file");
                    wake();
                }
                acked.retain(|_, (_, at)| at.elapsed() <= INSERT_TIMEOUT);
                listed.retain(|_, (_, at)| at.elapsed() <= INSERT_TIMEOUT);
                captured.retain(|_, (_, at)| at.elapsed() <= CAPTURE_TIMEOUT);
                let expired: Vec<u32> = pending
                    .iter()
                    .filter(|(_, (_, _, at))| at.elapsed() > INSERT_TIMEOUT)
                    .map(|(id, _)| *id)
                    .collect();
                for id in expired {
                    if let Some((tx, _, _)) = pending.remove(&id) {
                        let _ = tx.send(Err("the agent did not answer".into()));
                        wake();
                    }
                }
                if let Some(content) = local.poll().await
                    && up
                    && settings.borrow().sends()
                {
                    tracing::info!(?content, "clipboard to the target");
                    if let Content::Files(paths) = content {
                        queue_clip_files(paths, &mut queue, &mut batches, &mut next_batch, notes);
                    } else if let Err(e) = ep.send_clip(&content) {
                        tracing::warn!(%e, "cannot send clipboard item");
                    }
                }
            }
            _ = tokio::time::sleep_until(last_rx + SILENCE_TIMEOUT), if up => {
                return End::Lost("agent stopped answering".into(), true);
            }
            _ = ping.tick(), if up => {
                if let Err(e) = ep.ping() {
                    return End::Lost(e.to_string(), true);
                }
            }
            _ = status.tick(), if !up => {
                match session.bridge_status().await {
                    Ok(Some(st)) if st.agent.connected && st.agent.paired => {
                        let since = *agent_paired_since.get_or_insert_with(Instant::now);
                        if since.elapsed() > HANDSHAKE_TIMEOUT && connected_at.elapsed() > HANDSHAKE_TIMEOUT {
                            return End::Lost("agent connected but not answering".into(), false);
                        }
                    }
                    Ok(Some(_)) => {
                        agent_paired_since = None;
                        set(BridgeState::WaitingForAgent);
                    }
                    Ok(None) => return End::NotInstalled,
                    Err(_) => {}
                }
            }
            r = settings.changed() => {
                if r.is_err() {
                    return End::Lost("closed".into(), up);
                }
                if up && let Err(e) = ep.set_policy(settings.borrow().policy()) {
                    return End::Lost(e.to_string(), true);
                }
            }
        }
    }
}

fn host_name() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "Mac".into())
}

/// This Mac's clipboard: change detection, echo suppression, and the concealed check.
struct LocalClipboard {
    dedup: Dedup,
    last_count: Option<isize>,
}

impl LocalClipboard {
    /// What is on the clipboard now counts as seen: it is never sent.
    async fn new() -> LocalClipboard {
        let mut lc = LocalClipboard {
            dedup: Dedup::default(),
            last_count: Some(pasteboard::change_count()),
        };
        if let Some(c) = read_blocking().await {
            lc.dedup.observe_local(c.key());
        }
        lc
    }

    /// New local content to send, if the clipboard changed and it may be shared.
    async fn poll(&mut self) -> Option<Content> {
        let count = pasteboard::change_count();
        if self.last_count == Some(count) {
            return None;
        }
        self.last_count = Some(count);
        if pasteboard::concealed() {
            tracing::info!("clipboard content marked concealed/transient: not shared");
            return None;
        }
        let content = read_blocking().await?;
        self.dedup.observe_local(content.key()).then_some(content)
    }

    async fn apply(&mut self, content: Content) {
        self.dedup.applied(content.key());
        let r = tokio::task::spawn_blocking(move || {
            let mut clip = arboard::Clipboard::new()?;
            match content {
                Content::Text(t) => clip.set_text(t),
                Content::Files(paths) => clip.set().file_list(&paths),
                Content::Image {
                    width,
                    height,
                    rgba,
                } => clip.set_image(arboard::ImageData {
                    width: width as usize,
                    height: height as usize,
                    bytes: rgba.into(),
                }),
            }
        })
        .await;
        match r {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!(%e, "cannot set the clipboard"),
            Err(e) => tracing::warn!(%e, "clipboard task failed"),
        }
    }
}

async fn read_blocking() -> Option<Content> {
    tokio::task::spawn_blocking(|| {
        let mut clip = arboard::Clipboard::new().ok()?;
        if let Ok(files) = clip.get().file_list()
            && !files.is_empty()
        {
            return Some(Content::Files(files));
        }
        if let Ok(t) = clip.get_text()
            && !t.is_empty()
        {
            return Some(Content::Text(t));
        }
        let img = clip.get_image().ok()?;
        Some(Content::Image {
            width: img.width as u32,
            height: img.height as u32,
            rgba: img.bytes.into_owned(),
        })
    })
    .await
    .ok()
    .flatten()
}

#[cfg(target_os = "macos")]
mod pasteboard {
    use objc2_app_kit::NSPasteboard;

    /// Markers set by password managers (nspasteboard.org).
    const PRIVATE_TYPES: [&str; 2] = [
        "org.nspasteboard.ConcealedType",
        "org.nspasteboard.TransientType",
    ];

    pub fn change_count() -> isize {
        NSPasteboard::generalPasteboard().changeCount()
    }

    pub fn concealed() -> bool {
        let Some(types) = NSPasteboard::generalPasteboard().types() else {
            return false;
        };
        types
            .iter()
            .any(|t| PRIVATE_TYPES.contains(&t.to_string().as_str()))
    }
}

#[cfg(not(target_os = "macos"))]
mod pasteboard {
    use std::sync::atomic::{AtomicIsize, Ordering};

    /// No cheap change counter: report a change on every poll, the content key dedups.
    pub fn change_count() -> isize {
        static N: AtomicIsize = AtomicIsize::new(0);
        N.fetch_add(1, Ordering::Relaxed)
    }

    pub fn concealed() -> bool {
        false
    }
}

/// Queues files copied here to go to the target's clipboard, if they are small enough.
fn queue_clip_files(
    paths: Vec<PathBuf>,
    queue: &mut VecDeque<FileRequest>,
    batches: &mut HashMap<u32, Batch>,
    next_batch: &mut u32,
    notes: &mpsc::UnboundedSender<Note>,
) {
    let sized: Vec<(PathBuf, String, u64)> = cometdesk_clip::expand(&paths)
        .into_iter()
        .filter_map(|(p, name)| {
            let len = std::fs::metadata(&p).ok()?.len();
            Some((p, name, len))
        })
        .collect();
    let total: u64 = sized.iter().map(|(_, _, n)| n).sum();
    let skipped = if sized.is_empty() {
        Some("nothing to send")
    } else if sized.len() > cometdesk_clip::MAX_CLIP_FILES
        || total > cometdesk_clip::MAX_CLIP_FILES_BYTES
    {
        Some("too large to copy through the clipboard (use Send file)")
    } else {
        None
    };
    if let Some(why) = skipped {
        let _ = notes.send(Note::Info(format!("Copied files not sent: {why}")));
        return;
    }
    *next_batch += 1;
    batches.insert(
        *next_batch,
        Batch {
            ids: Vec::new(),
            remaining: sized.len(),
        },
    );
    for (path, name, size) in sized {
        let (reply, _) = oneshot::channel();
        queue.push_back(FileRequest {
            progress: Arc::new(TransferProgress {
                name,
                total: size,
                sent: AtomicU64::new(0),
                cancel: AtomicBool::new(false),
            }),
            path,
            reply,
            batch: Some(*next_batch),
            update: false,
        });
    }
}

/// Opens the file and tells the agent about it. On failure the request comes back with why.
fn start_file(ep: &mut Endpoint, req: FileRequest) -> Result<ActiveFile, (FileRequest, String)> {
    let purpose = if req.update {
        Purpose::Update
    } else if req.batch.is_some() {
        Purpose::Clipboard
    } else {
        Purpose::Save
    };
    match FileSender::start(ep, &req.path, &req.progress.name, purpose) {
        Ok(sender) => Ok(ActiveFile {
            sender,
            req,
            finished_at: None,
        }),
        Err(e) => Err((req, e)),
    }
}

/// Gives up on the rest of a clipboard batch: nothing is put on the target's clipboard.
fn drop_batch(
    batch: Option<u32>,
    batches: &mut HashMap<u32, Batch>,
    queue: &mut VecDeque<FileRequest>,
) {
    if let Some(b) = batch {
        batches.remove(&b);
        queue.retain(|r| r.batch != Some(b));
    }
}

/// Queues the next piece of `a`, or its end. `None`: the transfer is over (failed or cancelled).
fn stream_file(
    ep: &mut Endpoint,
    mut a: ActiveFile,
    batches: &mut HashMap<u32, Batch>,
    queue: &mut VecDeque<FileRequest>,
) -> Option<ActiveFile> {
    if a.req.progress.cancel.load(Ordering::Relaxed) {
        drop_batch(a.req.batch, batches, queue);
        a.sender.cancel(ep);
        let _ = a.req.reply.send(Err("cancelled".into()));
        return None;
    }
    match a.sender.pump(ep) {
        Ok(Pump::Sent(n)) => {
            a.req.progress.sent.fetch_add(n as u64, Ordering::Relaxed);
            Some(a)
        }
        Ok(Pump::Finished) => {
            a.finished_at = Some(Instant::now());
            Some(a)
        }
        Err(e) => {
            drop_batch(a.req.batch, batches, queue);
            a.fail(e);
            None
        }
    }
}

#[cfg(test)]
mod update_tests {
    use super::*;

    fn hello(protocol: u16, version: &str) -> Hello {
        Hello {
            protocol,
            version: version.into(),
            ..Hello::local("0", "t".into())
        }
    }

    #[test]
    fn older_agents_are_updated_newer_ones_are_not() {
        let ours = env!("CARGO_PKG_VERSION");
        assert!(agent_is_older(&hello(cometdesk_clip::PROTOCOL - 1, ours)));
        assert!(!agent_is_older(&hello(
            cometdesk_clip::PROTOCOL + 1,
            "0.0.1"
        )));
        assert!(!agent_is_older(&hello(cometdesk_clip::PROTOCOL, ours)));
        assert!(agent_is_older(&hello(cometdesk_clip::PROTOCOL, "0.0.1")));
        assert!(!agent_is_older(&hello(cometdesk_clip::PROTOCOL, "99.0.0")));
    }

    #[test]
    fn a_lower_uptime_means_a_restart() {
        assert!(restarted(Some(3600), 40));
        assert!(!restarted(Some(3600), 3610));
        assert!(!restarted(Some(100), 98));
        assert!(!restarted(None, 5));
    }
}
