//! Clipboard bridge client: syncs this Mac's clipboard with `glkvm-agent` on the target,
//! through the relay on the Comet (`/glkvm-bridge/ws`, docs/clipboard-bridge.md).
//!
//! One task per console. It reconnects on its own; every connection is a fresh end-to-end
//! encrypted session (the relay resets both sides when either reconnects). Only changes made
//! while connected are sent, never what was on the clipboard before, and content marked
//! concealed or transient by password managers is never sent.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use glkvm::Session;
use glkvm::api::bridge::BRIDGE_WS;
use glkvm::client::WsStream;
use glkvm_clip::{Content, Dedup, Endpoint, Error, Event, Hello, Policy, Role, Secret};
use tokio::runtime::Handle;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior, interval, sleep};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeState {
    Connecting,
    /// The relay is not installed on this device (`bridge/install.sh`).
    NotInstalled,
    /// Relay reached; no agent connected on the target.
    WaitingForAgent,
    /// The agent answered with a different pairing code.
    Mismatch,
    Connected(Hello),
    Error(String),
}

impl BridgeState {
    pub fn label(&self) -> String {
        match self {
            BridgeState::Connecting => "connecting…".into(),
            BridgeState::NotInstalled => "bridge not installed on this device".into(),
            BridgeState::WaitingForAgent => "waiting for the agent on the target".into(),
            BridgeState::Mismatch => "agent has a different pairing code".into(),
            BridgeState::Connected(h) => format!("connected to {} ({})", h.name, h.os),
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
        let task = rt.spawn(supervise(session, secret, settings_rx, state_tx, wake));
        ClipBridge {
            state,
            settings,
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
}

impl Drop for ClipBridge {
    fn drop(&mut self) {
        self.task.abort();
    }
}

enum End {
    Mismatch,
    NotInstalled,
    /// Reason, and whether a session had been established.
    Lost(String, bool),
}

async fn supervise(
    session: Session,
    secret: Secret,
    mut settings: watch::Receiver<Settings>,
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
                Ok(ws) => run(ws, &session, &secret, &mut settings, &mut local, &set).await,
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
        sleep(wait).await;
    }
}

async fn run(
    ws: WsStream,
    session: &Session,
    secret: &Secret,
    settings: &mut watch::Receiver<Settings>,
    local: &mut LocalClipboard,
    set: &impl Fn(BridgeState),
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
                    Err(e) => return End::Lost(e.to_string(), up),
                };
                for ev in events {
                    match ev {
                        Event::Ready(peer) => {
                            tracing::info!(agent = %peer.name, os = %peer.os, "clipboard bridge connected");
                            set(BridgeState::Connected(peer));
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
                        Event::Policy(_) => {}
                    }
                }
            }
            _ = poll.tick() => {
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
