//! Virtual media: images stored on the device, mounted on the target as a CD-ROM or USB drive
//! (the web app's "Image mounting"). Device-wide: the target and other viewers see changes.
//!
//! Background tasks write into [`Shared`] and wake the UI; the menu only reads it. The state
//! is fetched when the menu opens (if older than [`STALE`]) and after every action.

use crate::theme;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cometdesk::Session;
use cometdesk::api::msd::MsdState;
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
    fn run(&self, done: String, op: impl Future<Output = cometdesk::Result<()>> + Send + 'static) {
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

    /// Whether any virtual media image is currently mounted on the target.
    pub fn is_mounted(&self) -> bool {
        let s = self.lock();
        s.state.as_ref().and_then(MsdState::mounted).is_some()
    }

    /// Renders virtual media controls inside the side panel accordion.
    pub fn panel_ui(&mut self, ui: &mut egui::Ui) {
        let (state, working, upload) = {
            let s = self.lock();
            (s.state.clone(), s.working, s.upload.clone())
        };
        let stale = {
            let s = self.lock();
            s.fetched.is_none_or(|t| t.elapsed() > STALE)
        };
        if stale {
            self.refresh();
        }
        let Some(state) = state else {
            ui.label(egui::RichText::new("Reading the device…").color(theme::MUTED));
            return;
        };
        if !state.enabled {
            ui.label(
                egui::RichText::new("Virtual media is off on this device").color(theme::MUTED),
            );
            return;
        }

        ui.spacing_mut().item_spacing = egui::vec2(0.0, 12.0);

        // Mounted section
        theme::caption(ui, "MOUNTED IMAGE");
        let mount_card = theme::panel_card();
        mount_card.show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 8.0);
            match state.mounted() {
                Some(image) => {
                    let kind = if state.drive.cdrom {
                        "CD-ROM"
                    } else {
                        "USB drive"
                    };
                    ui.horizontal(|ui| {
                        theme::tag(ui, kind, theme::OK, theme::WELL);
                        ui.label(
                            egui::RichText::new(image)
                                .font(egui::FontId::new(13.0, theme::bold_family()))
                                .color(theme::TEXT),
                        );
                    });
                    if ui.add_enabled(!working, theme::ghost("Eject")).clicked() {
                        self.eject();
                    }
                }
                None => {
                    ui.label(egui::RichText::new("Nothing mounted").color(theme::MUTED));
                }
            }
        });

        // Images list section
        theme::caption(ui, "IMAGES ON DEVICE");
        if state.storage.images.is_empty() {
            let empty_card = theme::panel_card();
            empty_card.show(ui, |ui| {
                ui.label(egui::RichText::new("No images on the device").color(theme::MUTED));
            });
        }
        let can_mount = !working && !state.drive.connected && state.online && !state.busy;
        for (name, img) in &state.storage.images {
            let card = theme::panel_card();
            card.show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 8.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(name)
                            .font(egui::FontId::new(13.0, theme::bold_family()))
                            .color(theme::TEXT),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let size_text = if img.complete {
                            size(img.size)
                        } else {
                            "incomplete".to_string()
                        };
                        ui.label(theme::mono(size_text, 11.0).color(theme::MUTED));
                    });
                });

                if img.complete && can_mount {
                    let pair_w = (ui.available_width() - 8.0) * 0.5;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
                        let (rect_cd, resp_cd) =
                            ui.allocate_exact_size(egui::vec2(pair_w, 32.0), egui::Sense::click());
                        if ui.is_rect_visible(rect_cd) {
                            let fill = if resp_cd.hovered() {
                                theme::CARD_HOVER
                            } else {
                                theme::BG
                            };
                            ui.painter().rect(
                                rect_cd,
                                egui::CornerRadius::same(6),
                                fill,
                                egui::Stroke::new(1.0, theme::BORDER),
                                egui::StrokeKind::Inside,
                            );
                            let galley = ui.painter().layout_no_wrap(
                                "Mount CD".to_owned(),
                                egui::FontId::new(12.0, theme::bold_family()),
                                theme::TEXT,
                            );
                            ui.painter().galley(
                                rect_cd.center() - galley.size() * 0.5,
                                galley,
                                theme::TEXT,
                            );
                        }
                        if resp_cd.clicked() {
                            self.mount(name.clone(), true);
                        }

                        let (rect_usb, resp_usb) =
                            ui.allocate_exact_size(egui::vec2(pair_w, 32.0), egui::Sense::click());
                        if ui.is_rect_visible(rect_usb) {
                            let fill = if resp_usb.hovered() {
                                theme::CARD_HOVER
                            } else {
                                theme::BG
                            };
                            ui.painter().rect(
                                rect_usb,
                                egui::CornerRadius::same(6),
                                fill,
                                egui::Stroke::new(1.0, theme::BORDER),
                                egui::StrokeKind::Inside,
                            );
                            let galley = ui.painter().layout_no_wrap(
                                "Mount USB".to_owned(),
                                egui::FontId::new(12.0, theme::bold_family()),
                                theme::TEXT,
                            );
                            ui.painter().galley(
                                rect_usb.center() - galley.size() * 0.5,
                                galley,
                                theme::TEXT,
                            );
                        }
                        if resp_usb.clicked() {
                            self.mount(name.clone(), false);
                        }
                    });
                }

                let in_use = state.drive.connected && state.drive.image.as_deref() == Some(name);
                if !working && !in_use && ui.add(theme::ghost("Delete image")).clicked() {
                    self.remove(name.clone());
                }
            });
        }

        // Storage section
        theme::caption(ui, "STORAGE");
        let storage_card = theme::panel_card();
        storage_card.show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 8.0);
            match &upload {
                Some(u) => {
                    ui.label(
                        egui::RichText::new(format!("Uploading {}", u.name))
                            .font(egui::FontId::new(13.0, theme::bold_family()))
                            .color(theme::TEXT),
                    );
                    ui.add(
                        egui::ProgressBar::new(u.sent as f32 / u.total.max(1) as f32)
                            .show_percentage(),
                    );
                    if ui.add(theme::ghost("Cancel upload")).clicked() {
                        self.cancel_upload();
                    }
                }
                None => {
                    if theme::secondary_button(ui, "Upload image…", 34.0, false)
                        .on_hover_text("Or drop a file on the window")
                        .clicked()
                    {
                        self.pick();
                    }
                }
            }
            if let Some(free) = state.free() {
                ui.label(theme::mono(format!("{} free", size(free)), 11.0).color(theme::MUTED));
            }
        });
    }
}
