//! Outgoing and incoming file transfers, clipboard file batches, and agent update streaming.
//!
//! Owns the transfer queue, piece-by-piece pumping, and `FileOffer`/`FileData`/`FileEnd` events.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use cometdesk_clip::{Content, Endpoint, FileSender, Pump, Purpose};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Instant;

use super::run::Run;
use super::settings::Settings;
use super::state::BridgeState;
use super::supervise::End;
use super::types::{ActiveFile, Batch, FileRequest, Note, TransferProgress};

/// How long the agent gets to verify, sync and rename a file after its last byte.
const FILE_FINISH_TIMEOUT: Duration = Duration::from_secs(60);

impl<S: Fn(BridgeState), W: Fn() + Send + Sync> Run<'_, S, W> {
    pub(super) fn start_queued_files(&mut self, up: bool) {
        if up && self.active.is_none() {
            while let Some(req) = self.queue.pop_front() {
                match start_file(&mut self.ep, req) {
                    Ok(a) => {
                        self.active = Some(a);
                        break;
                    }
                    Err((req, e)) => {
                        drop_batch(req.batch, &mut self.batches, &mut self.queue);
                        let _ = req.reply.send(Err(e));
                    }
                }
            }
        }
    }

    pub(super) fn pump_active_file(&mut self) {
        if let Some(a) = self.active.take() {
            self.active = stream_file(&mut self.ep, a, &mut self.batches, &mut self.queue);
            // The progress bar needs redraws, but not one per piece.
            if self.active.is_none() || self.last_wake.elapsed() > Duration::from_millis(100) {
                self.last_wake = Instant::now();
                (self.wake)();
            }
        }
    }

    pub(super) fn on_file_request(&mut self, up: bool, req: FileRequest) {
        if up {
            self.queue.push_back(req);
        } else {
            let _ = req.reply.send(Err("the agent is not connected".into()));
        }
    }

    pub(super) fn on_file_result(
        &mut self,
        id: u32,
        result: Result<String, String>,
    ) -> Option<End> {
        if self.active.as_ref().is_some_and(|a| a.sender.id == id)
            && let Some(a) = self.active.take()
        {
            if a.req.update {
                let _ = self.notes.send(Note::Info(match &result {
                    Ok(_) => "The agent on the target was updated and is restarting".into(),
                    Err(e) => format!("Agent update failed: {e}"),
                }));
            }
            match &result {
                Ok(_) => {
                    if let Some(b) = a.req.batch
                        && let Some(batch) = self.batches.get_mut(&b)
                    {
                        batch.ids.push(id);
                        batch.remaining -= 1;
                        if batch.remaining == 0 {
                            let batch = self.batches.remove(&b).expect("just seen");
                            if let Err(e) = self.ep.send_files_ready(&batch.ids) {
                                return Some(End::Lost(e.to_string(), true));
                            }
                        }
                    }
                }
                Err(e) => {
                    // The agent dropped it; free what it kept for this id.
                    let _ = self.ep.send_file_cancel(id);
                    if a.req.batch.is_some() {
                        let _ = self
                            .notes
                            .send(Note::Info(format!("Copied files not sent: {e}")));
                    }
                    drop_batch(a.req.batch, &mut self.batches, &mut self.queue);
                }
            }
            let _ = a.req.reply.send(result);
            (self.wake)();
        }
        None
    }

    pub(super) fn on_file_offer(
        &mut self,
        id: u32,
        name: &str,
        size: u64,
        purpose: Purpose,
        settings: &watch::Receiver<Settings>,
    ) -> Option<End> {
        let refused = match purpose {
            Purpose::Clipboard if settings.borrow().receives() => {
                self.receiver.offer(id, name, size, purpose)
            }
            // Only what the user picked in the file browser.
            Purpose::Save if self.fetches > 0 => {
                self.fetches -= 1;
                let r = self.receiver.offer(id, name, size, purpose);
                if r.is_ok() {
                    self.downloading.insert(id);
                }
                r
            }
            _ => Err("the GUI does not take files from the target".to_string()),
        };
        if let Err(e) = refused
            && let Err(e) = self.ep.send_file_result(id, Err(e))
        {
            return Some(End::Lost(e.to_string(), true));
        }
        None
    }

    pub(super) fn on_file_data(&mut self, id: u32, data: &[u8]) -> Option<End> {
        if let Err(e) = self.receiver.data(id, data) {
            if self.downloading.remove(&id) {
                let _ = self.notes.send(Note::Info(format!("Download failed: {e}")));
                (self.wake)();
            }
            if let Err(e) = self.ep.send_file_result(id, Err(e)) {
                return Some(End::Lost(e.to_string(), true));
            }
        }
        None
    }

    pub(super) fn on_file_end(&mut self, id: u32, intact: bool) -> Option<End> {
        let result = self.receiver.end(id, intact);
        if self.downloading.remove(&id) {
            let _ = self.notes.send(Note::Info(match &result {
                Ok(path) => format!("Downloaded to {path}"),
                Err(e) => format!("Download failed: {e}"),
            }));
            (self.wake)();
        }
        if let Err(e) = self.ep.send_file_result(id, result) {
            return Some(End::Lost(e.to_string(), true));
        }
        None
    }

    pub(super) fn on_file_cancel(&mut self, id: u32) {
        self.receiver.cancel(id);
        if self.downloading.remove(&id) {
            let _ = self
                .notes
                .send(Note::Info("Download cancelled on the target".into()));
            (self.wake)();
        }
    }

    pub(super) async fn on_files_ready(&mut self, ids: &[u32]) {
        if let Some(paths) = self.receiver.take_staged(ids) {
            tracing::info!(count = paths.len(), "copied files from the target");
            self.local.apply(Content::Files(paths)).await;
        }
    }

    pub(super) fn expire_active_file(&mut self) {
        if self
            .active
            .as_ref()
            .and_then(|a| a.finished_at)
            .is_some_and(|t| t.elapsed() > FILE_FINISH_TIMEOUT)
            && let Some(a) = self.active.take()
        {
            a.fail("the agent did not confirm the file");
            (self.wake)();
        }
    }
}

/// Queues files copied here to go to the target's clipboard, if they are small enough.
pub(super) fn queue_clip_files(
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
