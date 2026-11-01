//! Public `ClipBridge` handle owned by a console session to talk to the background task.
//!
//! Spawns `supervise::supervise` and sends requests over unbounded channels.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

use cometdesk::Session;
use cometdesk::events::HidSender;
use cometdesk_clip::{DirTarget, PastePref, Secret};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use super::settings::Settings;
use super::state::BridgeState;
use super::supervise::supervise;
use super::types::{
    AckRequest, AckResult, CaptureRequest, CaptureResult, DirRequest, FileRequest, FileSend,
    InsertRequest, InsertResult, ListResult, Note, Request, TransferProgress,
};

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
