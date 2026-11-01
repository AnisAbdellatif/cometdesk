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

mod bridge;
mod settings;
mod state;
mod supervise;
mod types;
mod update;

pub use bridge::ClipBridge;
pub use settings::Settings;
pub use state::BridgeState;
pub use types::{
    AckResult, CaptureResult, FileSend, InsertResult, ListResult, Note, Request, TransferProgress,
};

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use cometdesk::Session;
use cometdesk::client::WsStream;
use cometdesk::events::HidSender;
use cometdesk_clip::{
    Content, Dedup, Endpoint, Error, Event, FileReceiver, FileSender, Hello, Info, InsertReply,
    Pump, Purpose, Role, Secret,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Instant, MissedTickBehavior, interval};
use tokio_tungstenite::tungstenite::Message;

use supervise::End;
use types::{
    AckRequest, ActiveFile, Batch, CaptureRequest, DirRequest, FileRequest, InsertRequest, Memory,
    paste_keys, restarted,
};
use update::queue_update;

const POLL: Duration = Duration::from_millis(300);
const PING: Duration = Duration::from_secs(15);
/// The agent answers every ping, so a session silent this long is dead (sleep, network change).
const SILENCE_TIMEOUT: Duration = Duration::from_secs(45);
const STATUS_EVERY: Duration = Duration::from_secs(5);
/// Agent connected per the relay, but no answer to our handshake for this long: assume a
/// dead connection and reset the session.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const INSERT_TIMEOUT: Duration = Duration::from_secs(5);
/// A 4K screenshot is several MB through the relay.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(20);

/// How long the agent gets to verify, sync and rename a file after its last byte.
const FILE_FINISH_TIMEOUT: Duration = Duration::from_secs(60);

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
    use super::types::agent_is_older;
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
