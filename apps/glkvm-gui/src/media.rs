//! Virtual media: images stored on the device, mounted on the target as a CD-ROM or USB drive
//! (the web app's "Image mounting"). Device-wide: the target and other viewers see changes.
//!
//! Background tasks write into [`Shared`] and wake the UI; the menu only reads it. The state
//! is fetched when the menu opens (if older than [`STALE`]) and after every action.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use glkvm::Session;
use glkvm::api::msd::MsdState;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use winit::event_loop::EventLoopProxy;

use crate::app::UserEvent;

const STALE: Duration = Duration::from_secs(3);

#[derive(Clone)]
struct Upload {
    name: String,
    sent: u64,
    total: u64,
}

#[derive(Default)]
struct Shared {
    state: Option<MsdState>,
    fetched: Option<Instant>,
    fetching: bool,
    /// A mount, eject or delete is in flight.
    working: bool,
    upload: Option<Upload>,
    /// Chosen in the file dialog, waiting for [`Media::poll`] to start the upload.
    picked: Option<PathBuf>,
}

pub struct Media {
    session: Session,
    rt: Handle,
    proxy: EventLoopProxy<UserEvent>,
    shared: Arc<Mutex<Shared>>,
    upload_task: Option<JoinHandle<()>>,
}

fn size(bytes: u64) -> String {
    const GIB: f64 = (1u64 << 30) as f64;
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.1} GiB", b / GIB)
    } else {
        format!("{:.0} MiB", b / (1u64 << 20) as f64)
    }
}

impl Media {
    pub fn new(session: Session, rt: Handle, proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            session,
            rt,
            proxy,
            shared: Arc::default(),
            upload_task: None,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap()
    }

    pub fn refresh(&self) {
        {
            let mut s = self.lock();
            if s.fetching {
                return;
            }
            s.fetching = true;
        }
        let (session, shared, proxy) = (
            self.session.clone(),
            self.shared.clone(),
            self.proxy.clone(),
        );
        self.rt.spawn(async move {
            let state = session.msd_state().await.ok();
            {
                let mut s = shared.lock().unwrap();
                s.fetching = false;
                s.fetched = Some(Instant::now());
                if state.is_some() {
                    s.state = state;
                }
            }
            let _ = proxy.send_event(UserEvent::Refresh);
        });
    }

    /// Runs one write, then reads the state back and reports the outcome.
    fn run(&self, done: String, op: impl Future<Output = glkvm::Result<()>> + Send + 'static) {
        self.lock().working = true;
        let (session, shared, proxy) = (
            self.session.clone(),
            self.shared.clone(),
            self.proxy.clone(),
        );
        self.rt.spawn(async move {
            let msg = match op.await {
                Ok(()) => done,
                Err(e) => format!("Virtual media: {e}"),
            };
            let state = session.msd_state().await.ok();
            {
                let mut s = shared.lock().unwrap();
                s.working = false;
                s.fetched = Some(Instant::now());
                if state.is_some() {
                    s.state = state;
                }
            }
            let _ = proxy.send_event(UserEvent::Notice(msg));
        });
    }

    fn mount(&self, image: String, cdrom: bool) {
        let session = self.session.clone();
        let done = format!("Mounted {image}");
        self.run(done, async move { session.msd_mount(&image, cdrom).await });
    }

    fn eject(&self) {
        let session = self.session.clone();
        self.run("Ejected".into(), async move {
            session.msd_set_connected(false).await
        });
    }

    fn remove(&self, image: String) {
        let session = self.session.clone();
        let done = format!("Deleted {image}");
        self.run(done, async move { session.msd_remove(&image).await });
    }

    /// Asks for a file in the background; [`Self::poll`] starts the upload.
    fn pick(&self) {
        let (shared, proxy) = (self.shared.clone(), self.proxy.clone());
        self.rt.spawn(async move {
            let file = rfd::AsyncFileDialog::new()
                .set_title("Upload an image to the device")
                .pick_file()
                .await;
            if let Some(f) = file {
                shared.lock().unwrap().picked = Some(f.path().to_owned());
            }
            let _ = proxy.send_event(UserEvent::Refresh);
        });
    }

    /// Starts an upload chosen in the file dialog.
    pub fn poll(&mut self) {
        let picked = self.lock().picked.take();
        if let Some(path) = picked {
            self.upload(path);
        }
    }

    /// Files dropped on the window: uploads the first.
    pub fn dropped(&mut self, paths: Vec<PathBuf>) {
        if let Some(path) = paths.into_iter().next() {
            self.upload(path);
        }
    }

    fn notice(&self, msg: String) {
        let _ = self.proxy.send_event(UserEvent::Notice(msg));
    }

    fn upload(&mut self, path: PathBuf) {
        let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
            return self.notice("Cannot upload: the file name isn't valid UTF-8".into());
        };
        let total = match std::fs::metadata(&path) {
            Ok(m) if m.is_file() => m.len(),
            _ => return self.notice(format!("Cannot upload {name}: not a file")),
        };
        {
            let mut s = self.lock();
            if s.upload.is_some() {
                drop(s);
                return self.notice("An upload is already running".into());
            }
            if let Some(state) = &s.state {
                let problem = if state.storage.images.contains_key(&name) {
                    Some(format!(
                        "{name} already exists on the device; delete it first"
                    ))
                } else if state.free().is_some_and(|free| total > free) {
                    Some(format!(
                        "{name} ({}) doesn't fit: {} free",
                        size(total),
                        size(state.free().unwrap())
                    ))
                } else {
                    None
                };
                if let Some(msg) = problem {
                    drop(s);
                    return self.notice(msg);
                }
            }
            s.upload = Some(Upload {
                name: name.clone(),
                sent: 0,
                total,
            });
        }
        let (session, shared, proxy) = (
            self.session.clone(),
            self.shared.clone(),
            self.proxy.clone(),
        );
        self.upload_task = Some(self.rt.spawn(async move {
            let last = AtomicU64::new(0);
            let progress = {
                let (shared, proxy) = (shared.clone(), proxy.clone());
                move |sent: u64, total: u64| {
                    if let Some(u) = shared.lock().unwrap().upload.as_mut() {
                        u.sent = sent;
                    }
                    let pct = sent * 100 / total.max(1);
                    if pct != last.swap(pct, Ordering::Relaxed) {
                        let _ = proxy.send_event(UserEvent::Refresh);
                    }
                }
            };
            let msg = match session.msd_upload(&path, &name, progress).await {
                Ok(()) => format!("Uploaded {name}"),
                Err(e) => format!("Upload of {name} failed: {e}"),
            };
            let state = session.msd_state().await.ok();
            {
                let mut s = shared.lock().unwrap();
                s.upload = None;
                s.fetched = Some(Instant::now());
                if state.is_some() {
                    s.state = state;
                }
            }
            let _ = proxy.send_event(UserEvent::Notice(msg));
        }));
    }

    fn cancel_upload(&mut self) {
        if let Some(t) = self.upload_task.take() {
            t.abort();
        }
        self.lock().upload = None;
        self.refresh();
        self.notice("Upload cancelled; delete the incomplete image from the Media menu".into());
    }

    /// Stops an upload that is still running (the console is closing).
    pub fn shutdown(&mut self) {
        if let Some(t) = self.upload_task.take() {
            t.abort();
        }
    }

    /// The "Media" toolbar menu.
    pub fn menu(&mut self, ui: &mut egui::Ui) {
        let (state, working, upload) = {
            let s = self.lock();
            (s.state.clone(), s.working, s.upload.clone())
        };
        let title = match &upload {
            Some(u) => format!("Media {}%", u.sent * 100 / u.total.max(1)),
            None if state.as_ref().and_then(MsdState::mounted).is_some() => "Media ●".into(),
            None => "Media".into(),
        };
        ui.menu_button(title, |ui| {
            let stale = {
                let s = self.lock();
                s.fetched.is_none_or(|t| t.elapsed() > STALE)
            };
            if stale {
                self.refresh();
            }
            let Some(state) = state else {
                ui.label("Reading the device…");
                return;
            };
            if !state.enabled {
                ui.label("Virtual media is off on this device");
                return;
            }
            let connected = state.drive.connected;
            match state.mounted() {
                Some(image) => {
                    let kind = if state.drive.cdrom {
                        "CD-ROM"
                    } else {
                        "USB drive"
                    };
                    ui.label(format!("Mounted: {image} ({kind})"));
                    if ui
                        .add_enabled(!working, egui::Button::new("Eject"))
                        .clicked()
                    {
                        self.eject();
                        ui.close();
                    }
                }
                None => {
                    ui.label("Nothing mounted");
                }
            }
            ui.separator();
            if state.storage.images.is_empty() {
                ui.label("No images on the device");
            }
            let can_mount = !working && !connected && state.online && !state.busy;
            for (name, img) in &state.storage.images {
                let label = if img.complete {
                    format!("{name}  ({})", size(img.size))
                } else {
                    format!("{name}  (incomplete)")
                };
                ui.menu_button(label, |ui| {
                    if img.complete {
                        for (text, cdrom) in
                            [("Mount as CD-ROM", true), ("Mount as USB drive", false)]
                        {
                            let tip = if connected {
                                "Eject the mounted image first"
                            } else {
                                "Not ready"
                            };
                            if ui
                                .add_enabled(can_mount, egui::Button::new(text))
                                .on_disabled_hover_text(tip)
                                .clicked()
                            {
                                self.mount(name.clone(), cdrom);
                                ui.close();
                            }
                        }
                    }
                    let in_use =
                        state.drive.connected && state.drive.image.as_deref() == Some(name);
                    ui.add_enabled_ui(!working && !in_use, |ui| {
                        ui.menu_button("Delete from device", |ui| {
                            if ui.button(format!("Really delete {name}")).clicked() {
                                self.remove(name.clone());
                                ui.close();
                            }
                        });
                    });
                });
            }
            ui.separator();
            match &upload {
                Some(u) => {
                    ui.label(format!("Uploading {}", u.name));
                    ui.add(
                        egui::ProgressBar::new(u.sent as f32 / u.total.max(1) as f32)
                            .show_percentage(),
                    );
                    if ui.button("Cancel upload").clicked() {
                        self.cancel_upload();
                        ui.close();
                    }
                }
                None => {
                    if ui
                        .button("Upload image…")
                        .on_hover_text("Or drop a file on the window")
                        .clicked()
                    {
                        self.pick();
                        ui.close();
                    }
                }
            }
            if let Some(free) = state.free() {
                ui.weak(format!("{} free", size(free)));
            }
        });
    }
}
