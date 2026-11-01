//! Connected WebSocket session loop (`run`) and its per-session state (`Run`).
//!
//! Multiplexes Noise frames, request channels, file streaming, clipboard polling, and keepalive pings.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

use cometdesk::Session;
use cometdesk::client::WsStream;
use cometdesk::events::HidSender;
use cometdesk_clip::{Endpoint, Error, Event, FileReceiver, Hello, Role, Secret};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Instant, MissedTickBehavior, interval};
use tokio_tungstenite::tungstenite::Message;

use super::LocalClipboard;
use super::settings::Settings;
use super::state::BridgeState;
use super::supervise::End;
use super::types::{
    AckRequest, AckResult, ActiveFile, Batch, CaptureRequest, CaptureResult, DirRequest,
    FileRequest, InsertRequest, InsertResult, ListResult, Memory, Note,
};

const POLL: Duration = Duration::from_millis(300);
const PING: Duration = Duration::from_secs(15);
/// The agent answers every ping, so a session silent this long is dead (sleep, network change).
const SILENCE_TIMEOUT: Duration = Duration::from_secs(45);
const STATUS_EVERY: Duration = Duration::from_secs(5);
/// Agent connected per the relay, but no answer to our handshake for this long: assume a
/// dead connection and reset the session.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) struct Run<'a, S, W> {
    pub(super) ep: Endpoint,
    pub(super) notes: &'a mpsc::UnboundedSender<Note>,
    pub(super) local: &'a mut LocalClipboard,
    pub(super) memory: &'a mut Memory,
    pub(super) set: &'a S,
    pub(super) wake: &'a W,
    // Inserts waiting for the agent's reply: id -> (reply, keyboard, asked at).
    pub(super) pending: HashMap<u32, (oneshot::Sender<InsertResult>, HidSender, Instant)>,
    pub(super) captured: HashMap<u32, (oneshot::Sender<CaptureResult>, Instant)>,
    pub(super) queue: VecDeque<FileRequest>,
    pub(super) active: Option<ActiveFile>,
    pub(super) batches: HashMap<u32, Batch>,
    pub(super) next_batch: u32,
    pub(super) receiver: FileReceiver,
    pub(super) last_wake: Instant,
    pub(super) acked: HashMap<u32, (oneshot::Sender<AckResult>, Instant)>,
    pub(super) listed: HashMap<u32, (oneshot::Sender<ListResult>, Instant)>,
    // Files the user asked to download that have not been offered or refused yet, and the
    // ones being received now.
    pub(super) fetches: u32,
    pub(super) fetch_acks: HashSet<u32>,
    pub(super) downloading: HashSet<u32>,
}

impl<'a, S: Fn(BridgeState), W: Fn() + Send + Sync> Run<'a, S, W> {
    fn new(
        ep: Endpoint,
        notes: &'a mpsc::UnboundedSender<Note>,
        local: &'a mut LocalClipboard,
        memory: &'a mut Memory,
        set: &'a S,
        wake: &'a W,
    ) -> Self {
        Self {
            ep,
            notes,
            local,
            memory,
            set,
            wake,
            pending: HashMap::new(),
            captured: HashMap::new(),
            queue: VecDeque::new(),
            active: None,
            batches: HashMap::new(),
            next_batch: 0,
            receiver: FileReceiver::new(std::env::temp_dir().join("cometdesk-clip")),
            last_wake: Instant::now(),
            acked: HashMap::new(),
            listed: HashMap::new(),
            fetches: 0,
            fetch_acks: HashSet::new(),
            downloading: HashSet::new(),
        }
    }

    async fn handle_event(
        &mut self,
        ev: Event,
        settings: &watch::Receiver<Settings>,
    ) -> Option<End> {
        match ev {
            Event::Ready(peer) => return self.on_ready(peer, settings),
            Event::Mismatch(peer) => return self.on_mismatch(peer),
            Event::Clip(content) => self.on_clip(content, settings).await,
            Event::InsertReply { id, reply } => return self.on_insert_reply(id, reply),
            Event::Info(info) => self.on_info(info),
            Event::Screenshot { id, png } => self.on_screenshot(id, png),
            Event::CaptureFailed { id, reason } => self.on_capture_failed(id, reason),
            Event::Ack { id, result } => self.on_ack(id, result),
            Event::DirList { id, result } => self.on_dir_list(id, result),
            Event::FileResult { id, result } => return self.on_file_result(id, result),
            // The agent sends files when something was copied on the target.
            Event::FileOffer {
                id,
                name,
                size,
                purpose,
            } => return self.on_file_offer(id, &name, size, purpose, settings),
            Event::FileData { id, data } => return self.on_file_data(id, &data),
            Event::FileEnd { id, intact } => return self.on_file_end(id, intact),
            Event::FileCancel { id } => self.on_file_cancel(id),
            Event::FilesReady { ids } => self.on_files_ready(&ids).await,
            Event::Notify { title, body } => self.on_notify(title, body),
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
        None
    }

    async fn on_poll_tick(&mut self, up: bool, settings: &watch::Receiver<Settings>) {
        self.expire_active_file();
        self.expire_acks_and_dirs();
        self.expire_captures();
        self.expire_inserts();
        self.poll_clipboard(up, settings).await;
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run(
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
    let ep = match Endpoint::new(Role::Client, secret, hello) {
        Ok(ep) => ep,
        Err(e) => return End::Lost(e.to_string(), false),
    };
    set(BridgeState::WaitingForAgent);
    let connected_at = Instant::now();
    let mut agent_paired_since: Option<Instant> = None;
    let mut state = Run::new(ep, notes, local, memory, set, wake);
    let mut poll = interval(POLL);
    let mut ping = interval(PING);
    let mut status = interval(STATUS_EVERY);
    let mut last_rx = Instant::now();
    for t in [&mut poll, &mut ping, &mut status] {
        t.set_missed_tick_behavior(MissedTickBehavior::Delay);
    }
    loop {
        if state.ep.has_outgoing()
            && let Err(e) = sink
                .send(Message::Binary(state.ep.take_outgoing().into()))
                .await
        {
            return End::Lost(e.to_string(), state.ep.is_ready());
        }
        let up = state.ep.is_ready();
        state.start_queued_files(up);
        tokio::select! {
            // One piece at a time, so pings, the clipboard and replies keep flowing.
            _ = tokio::task::yield_now(), if state.active.as_ref().is_some_and(|a| a.finished_at.is_none()) => {
                state.pump_active_file();
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
                let events = match state.ep.receive(&data) {
                    Ok(ev) => ev,
                    Err(Error::Handshake) => return End::Mismatch,
                    Err(Error::Version(v)) => return End::AgentOutdated(v),
                    Err(e) => return End::Lost(e.to_string(), up),
                };
                for ev in events {
                    if let Some(end) = state.handle_event(ev, settings).await {
                        return end;
                    }
                }
            }
            Some(req) = inserts.recv() => state.on_insert_request(up, req),
            Some(req) = captures.recv() => state.on_capture_request(up, req),
            Some(req) = files.recv() => state.on_file_request(up, req),
            Some(req) = dirs.recv() => state.on_dir_request(up, req),
            Some(req) = acks.recv() => state.on_ack_request(up, req),
            _ = poll.tick() => state.on_poll_tick(up, settings).await,
            _ = tokio::time::sleep_until(last_rx + SILENCE_TIMEOUT), if up => {
                return End::Lost("agent stopped answering".into(), true);
            }
            _ = ping.tick(), if up => {
                if let Err(e) = state.ep.ping() {
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
                if up && let Err(e) = state.ep.set_policy(settings.borrow().policy()) {
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
