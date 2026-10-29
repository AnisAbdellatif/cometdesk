//! Sends the files copied on the target to the GUI, one at a time, a piece per loop turn so
//! the connection keeps being served. The GUI stages them and puts them on its clipboard
//! once [`Event::FilesReady`](cometdesk_clip::Event::FilesReady) says the batch is complete.

use std::collections::VecDeque;
use std::path::PathBuf;

use cometdesk_clip::{Endpoint, FileSender, MAX_CLIP_FILES, MAX_CLIP_FILES_BYTES, Pump, Purpose};

use crate::log;

#[derive(Default)]
pub struct Outbox {
    queue: VecDeque<PathBuf>,
    active: Option<FileSender>,
    /// The active file is fully sent; waiting for the GUI's verdict.
    waiting: bool,
    done: Vec<u32>,
}

impl Outbox {
    /// Replaces whatever was being sent. Folders and files over the limits are skipped, and the
    /// rest is not sent at all if nothing is left.
    pub fn start(&mut self, ep: &mut Endpoint, paths: Vec<PathBuf>) {
        self.stop(ep);
        let files: Vec<PathBuf> = paths
            .into_iter()
            .filter(|p| {
                std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() <= MAX_CLIP_FILES_BYTES)
            })
            .take(MAX_CLIP_FILES)
            .collect();
        let total: u64 = files
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum();
        if files.is_empty() || total > MAX_CLIP_FILES_BYTES {
            log!("copied files not sent: folders, or more than the size limit");
            return;
        }
        self.queue = files.into();
        self.next(ep);
    }

    /// Whether [`Outbox::pump`] has something to do, so the caller should poll quickly.
    pub fn busy(&self) -> bool {
        self.active.is_some() && !self.waiting
    }

    pub fn stop(&mut self, ep: &mut Endpoint) {
        if let Some(a) = self.active.take() {
            a.cancel(ep);
        }
        *self = Outbox::default();
    }

    fn next(&mut self, ep: &mut Endpoint) {
        if let Some(path) = self.queue.pop_front() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_owned();
            match FileSender::start(ep, &path, &name, Purpose::Clipboard) {
                Ok(s) => {
                    self.active = Some(s);
                    self.waiting = false;
                }
                Err(e) => {
                    log!("cannot send {}: {e}", path.display());
                    self.stop(ep);
                }
            }
            return;
        }
        // Nothing left to start: the batch is complete.
        if !self.done.is_empty() {
            let ids = std::mem::take(&mut self.done);
            if let Err(e) = ep.send_files_ready(&ids) {
                log!("cannot finish the file batch: {e}");
            }
        }
    }

    /// Queues the next piece of the active file.
    pub fn pump(&mut self, ep: &mut Endpoint) {
        let Some(a) = self.active.as_mut().filter(|_| !self.waiting) else {
            return;
        };
        match a.pump(ep) {
            Ok(Pump::Sent(_)) => {}
            Ok(Pump::Finished) => self.waiting = true,
            Err(e) => {
                log!("copied file failed: {e}");
                self.active = None;
                self.stop(ep);
            }
        }
    }

    /// The GUI's verdict on file `id`.
    pub fn result(&mut self, ep: &mut Endpoint, id: u32, result: Result<String, String>) {
        if self.active.as_ref().map(|a| a.id) != Some(id) {
            return;
        }
        self.active = None;
        match result {
            Ok(_) => {
                self.done.push(id);
                self.next(ep);
            }
            Err(e) => {
                log!("the GUI refused a copied file: {e}");
                self.stop(ep);
            }
        }
    }
}
