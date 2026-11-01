//! Queues an in-band `cometdesk-agent` binary update when a connected target runs an older build.
//!
//! Called from `run.rs` on `Event::Ready` and `Event::Mismatch`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

use cometdesk_clip::Hello;
use tokio::sync::{mpsc, oneshot};

use super::types::{FileRequest, Memory, Note, TransferProgress, agent_is_older};

/// Queues the agent program this app ships for `peer`, if `peer` is older and no update was
/// sent to that version yet. Whether anything was queued.
pub(super) fn queue_update(
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
