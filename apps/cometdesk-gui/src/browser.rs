//! A window listing the target's files, to download them to this Mac's Downloads folder.
//! The listing comes from the agent ([`ClipBridge::list_dir`]); a download is a request the
//! agent answers by streaming the file back (see `clipsync`).

use cometdesk_clip::{DirTarget, Listing};
use tokio::runtime::Handle;
use tokio::sync::oneshot::{self, error::TryRecvError};
use winit::event_loop::EventLoopProxy;

use crate::app::UserEvent;
use crate::clipsync::{ClipBridge, ListResult, Request};

#[derive(Default)]
pub struct Browser {
    open: bool,
    listing: Option<Listing>,
    pending: Option<oneshot::Receiver<ListResult>>,
    error: Option<String>,
    show_hidden: bool,
    filter: String,
}

impl Browser {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens the window on the target's home folder.
    pub fn open(&mut self, clip: &ClipBridge) {
        self.open = true;
        self.go(clip, DirTarget::Home);
    }

    pub fn close(&mut self) {
        *self = Browser::default();
    }

    fn go(&mut self, clip: &ClipBridge, target: DirTarget) {
        self.pending = Some(clip.list_dir(target));
        self.error = None;
    }

    fn poll(&mut self) {
        let Some(rx) = self.pending.as_mut() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(listing)) => {
                self.listing = Some(listing);
                self.pending = None;
            }
            Ok(Err(e)) => {
                self.error = Some(e);
                self.pending = None;
            }
            Err(TryRecvError::Closed) => {
                self.error = Some("the agent disconnected".into());
                self.pending = None;
            }
            Err(TryRecvError::Empty) => {}
        }
    }

    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        clip: &ClipBridge,
        rt: &Handle,
        proxy: &EventLoopProxy<UserEvent>,
    ) {
        if !self.open {
            return;
        }
        self.poll();
        let mut open = true;
        let mut go: Option<DirTarget> = None;
        let mut fetch: Option<(String, String)> = None;
        egui::Window::new("Files on the target")
            .open(&mut open)
            .default_size([460.0, 420.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Home").clicked() {
                        go = Some(DirTarget::Home);
                    }
                    let here = self.listing.as_ref().map(|l| l.path.clone());
                    if ui
                        .add_enabled(
                            here.as_deref().is_some_and(|p| !p.is_empty()),
                            egui::Button::new("Up"),
                        )
                        .clicked()
                    {
                        go = here.clone().map(DirTarget::Parent);
                    }
                    if ui.button("Drives").clicked() {
                        go = Some(DirTarget::Roots);
                    }
                    ui.checkbox(&mut self.show_hidden, "Hidden");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter)
                            .hint_text("Filter")
                            .desired_width(120.0),
                    );
                    if self.pending.is_some() {
                        ui.spinner();
                    }
                });
                if let Some(l) = &self.listing {
                    ui.label(if l.path.is_empty() {
                        "Drives"
                    } else {
                        l.path.as_str()
                    });
                }
                if let Some(e) = &self.error {
                    ui.colored_label(crate::theme::ERR, e);
                }
                ui.separator();
                let Some(l) = &self.listing else {
                    return;
                };
                let filter = self.filter.to_lowercase();
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        for e in l.entries.iter().filter(|e| {
                            (self.show_hidden || !e.name.starts_with('.'))
                                && (filter.is_empty() || e.name.to_lowercase().contains(&filter))
                        }) {
                            ui.horizontal(|ui| {
                                if e.dir {
                                    if ui.link(format!("{}/", e.name)).clicked() {
                                        go = Some(if l.path.is_empty() {
                                            DirTarget::Open(e.name.clone())
                                        } else {
                                            DirTarget::Child {
                                                dir: l.path.clone(),
                                                name: e.name.clone(),
                                            }
                                        });
                                    }
                                } else {
                                    if ui.button("Download").clicked() {
                                        fetch = Some((l.path.clone(), e.name.clone()));
                                    }
                                    ui.label(&e.name);
                                    ui.weak(size(e.size));
                                }
                            });
                        }
                        if l.truncated {
                            ui.weak("More entries than fit: use the filter or open a subfolder");
                        }
                    });
            });
        if !open {
            self.close();
            return;
        }
        if let Some(t) = go {
            self.go(clip, t);
        }
        if let Some((dir, name)) = fetch {
            let rx = clip.request(Request::Fetch {
                dir,
                name: name.clone(),
            });
            let proxy = proxy.clone();
            rt.spawn(async move {
                let msg = match rx.await {
                    Ok(Ok(())) => format!("Downloading {name}"),
                    Ok(Err(e)) => format!("Cannot download {name}: {e}"),
                    Err(_) => format!("Cannot download {name}: the agent disconnected"),
                };
                let _ = proxy.send_event(UserEvent::Notice(msg));
            });
        }
    }
}

fn size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.0} KB", b as f64 / 1024.0),
        b => format!("{b} B"),
    }
}
