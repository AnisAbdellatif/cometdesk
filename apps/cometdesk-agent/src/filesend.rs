//! Sends files to the GUI, one at a time, a piece per loop turn so the connection keeps being
//! served: the files copied on the target (the GUI stages them and puts them on its clipboard
//! once [`Event::FilesReady`](cometdesk_clip::Event::FilesReady) says the batch is complete),
//! and files the GUI asked to fetch (saved to its Downloads folder).

use std::collections::VecDeque;
use std::path::PathBuf;

use cometdesk_clip::{
    Endpoint, FileSender, MAX_CLIP_FILES, MAX_CLIP_FILES_BYTES, Pump, Purpose, expand,
};

use crate::log;

struct Job {
    path: PathBuf,
    /// The name to offer it under: `folder/sub/file` for part of a copied folder.
    name: String,
    purpose: Purpose,
}

#[derive(Default)]
pub struct Outbox {
    queue: VecDeque<Job>,
    active: Option<(FileSender, Purpose)>,
    /// The active file is fully sent; waiting for the GUI's verdict.
    waiting: bool,
    done: Vec<u32>,
}

impl Outbox {
    /// Replaces the clipboard files being sent. Files and folders over the limits are not sent
    /// at all.
    pub fn start(&mut self, ep: &mut Endpoint, paths: Vec<PathBuf>) {
        self.drop_clipboard(ep);
        let files = expand(&paths);
        let total: u64 = files
            .iter()
            .filter_map(|(p, _)| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum();
        let too_big = files
            .iter()
            .any(|(p, _)| std::fs::metadata(p).is_ok_and(|m| m.len() > MAX_CLIP_FILES_BYTES));
        if files.is_empty()
            || files.len() > MAX_CLIP_FILES
            || total > MAX_CLIP_FILES_BYTES
            || too_big
        {
            log!("copied files not sent: nothing to send, or over the size or count limit");
            return;
        }
        self.queue.extend(files.into_iter().map(|(path, name)| Job {
            path,
            name,
            purpose: Purpose::Clipboard,
        }));
        self.next(ep);
    }

    /// Queues a file the GUI asked for.
    pub fn fetch(&mut self, ep: &mut Endpoint, path: PathBuf) {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_owned();
        self.queue.push_back(Job {
            path,
            name,
            purpose: Purpose::Save,
        });
        self.next(ep);
    }

    /// Whether [`Outbox::pump`] has something to do, so the caller should poll quickly.
    pub fn busy(&self) -> bool {
        self.active.is_some() && !self.waiting
    }

    /// Gives up on the clipboard files, leaving fetches alone.
    fn drop_clipboard(&mut self, ep: &mut Endpoint) {
        if self
            .active
            .as_ref()
            .is_some_and(|(_, p)| *p == Purpose::Clipboard)
            && let Some((a, _)) = self.active.take()
        {
            a.cancel(ep);
            self.waiting = false;
        }
        self.queue.retain(|j| j.purpose != Purpose::Clipboard);
        self.done.clear();
    }

    /// Starts the next file if none is in flight.
    fn next(&mut self, ep: &mut Endpoint) {
        while self.active.is_none() {
            let Some(job) = self.queue.pop_front() else {
                return;
            };
            match FileSender::start(ep, &job.path, &job.name, job.purpose) {
                Ok(s) => {
                    self.active = Some((s, job.purpose));
                    self.waiting = false;
                }
                Err(e) => {
                    log!("cannot send {}: {e}", job.path.display());
                    if job.purpose == Purpose::Clipboard {
                        self.drop_clipboard(ep);
                    }
                }
            }
        }
    }

    /// Queues the next piece of the active file.
    pub fn pump(&mut self, ep: &mut Endpoint) {
        let Some((a, purpose)) = self.active.as_mut().filter(|_| !self.waiting) else {
            return;
        };
        let purpose = *purpose;
        match a.pump(ep) {
            Ok(Pump::Sent(_)) => {}
            Ok(Pump::Finished) => self.waiting = true,
            Err(e) => {
                log!("file failed: {e}");
                self.active = None;
                if purpose == Purpose::Clipboard {
                    self.drop_clipboard(ep);
                }
                self.next(ep);
            }
        }
    }

    /// The GUI's verdict on file `id`.
    pub fn result(&mut self, ep: &mut Endpoint, id: u32, result: Result<String, String>) {
        let Some((_, purpose)) = self.active.as_ref().filter(|(a, _)| a.id == id) else {
            return;
        };
        let purpose = *purpose;
        self.active = None;
        match (result, purpose) {
            (Ok(_), Purpose::Clipboard) => {
                self.done.push(id);
                // The batch is complete once no clipboard file is left to send.
                if !self.queue.iter().any(|j| j.purpose == Purpose::Clipboard) {
                    let ids = std::mem::take(&mut self.done);
                    if let Err(e) = ep.send_files_ready(&ids) {
                        log!("cannot finish the file batch: {e}");
                    }
                }
            }
            (Ok(_), _) => {}
            (Err(e), Purpose::Clipboard) => {
                log!("the GUI refused a copied file: {e}");
                self.drop_clipboard(ep);
            }
            (Err(e), _) => log!("the GUI refused a file: {e}"),
        }
        self.next(ep);
    }
}
