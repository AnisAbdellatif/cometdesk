//! Clipboard bridge client: syncs this Mac's clipboard with `glkvm-agent` on the target,
//! through the relay on the Comet (`/glkvm-bridge/ws`, docs/clipboard-bridge.md).
//!
//! One task per console. It reconnects on its own; every connection is a fresh end-to-end
//! encrypted session (the relay resets both sides when either reconnects). Only changes made
//! while connected are sent, never what was on the clipboard before, and content marked
//! concealed or transient by password managers is never sent.
//!
//! It also inserts text on the target through the agent ([`ClipBridge::insert`]): in one
//! step instead of a stream of key presses that a window switch could misdirect. When the
//! agent asks for a paste keystroke, this task sends it over the KVM's keyboard.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use glkvm::Session;
use glkvm::api::bridge::BRIDGE_WS;
use glkvm::client::WsStream;
use glkvm::events::HidSender;
use glkvm_clip::{
    Content, Dedup, Endpoint, Error, Event, Hello, Info, InsertReply, PasteKey, PastePref, Policy,
    Role, Secret,
};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior, interval};
use tokio_tungstenite::tungstenite::Message;

use crate::store::ClipDirection;

const POLL: Duration = Duration::from_millis(300);
const PING: Duration = Duration::from_secs(15);
const STATUS_EVERY: Duration = Duration::from_secs(5);
/// Agent connected per the relay, but no answer to our handshake for this long: assume a
/// dead connection and reset the session.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(15);
const RETRY_SLOW: Duration = Duration::from_secs(30);
const INSERT_TIMEOUT: Duration = Duration::from_secs(5);
/// A 4K screenshot is several MB through the relay.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(20);

/// Outcome of an insert: a short description, or why it failed.
pub type InsertResult = Result<String, String>;

/// PNG bytes of the target's screen, or why there are none.
pub type CaptureResult = Result<Vec<u8>, String>;

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
        let task = rt.spawn(supervise(
            session,
            secret,
            settings_rx,
            inserts_rx,
            captures_rx,
            state_tx,
            wake,
        ));
        ClipBridge {
            state,
            settings,
            inserts,
            captures,
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

async fn supervise(
    session: Session,
    secret: Secret,
    mut settings: watch::Receiver<Settings>,
    mut inserts: mpsc::UnboundedReceiver<InsertRequest>,
    mut captures: mpsc::UnboundedReceiver<CaptureRequest>,
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
                        &mut local,
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
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => break,
                Some(req) = inserts.recv() => {
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
    local: &mut LocalClipboard,
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
    let mut poll = interval(POLL);
    let mut ping = interval(PING);
    let mut status = interval(STATUS_EVERY);
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
        tokio::select! {
            msg = stream.next() => {
                let data = match msg {
                    Some(Ok(Message::Binary(b))) => b,
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
                            set(BridgeState::Connected(peer, Info::default()));
                            if let Err(e) = ep.set_policy(settings.borrow().policy()) {
                                return End::Lost(e.to_string(), true);
                            }
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
                        Event::Policy(_)
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
            _ = poll.tick() => {
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
                    if let Err(e) = ep.send_clip(&content) {
                        tracing::warn!(%e, "cannot send clipboard item");
                    }
                }
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
