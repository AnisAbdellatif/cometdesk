//! A window listing the target's files, to download them to this Mac's Downloads folder.
//! The listing comes from the agent ([`ClipBridge::list_dir`]); a download is a request the
//! agent answers by streaming the file back (see `clipsync`).

use cometdesk_clip::{DirTarget, Listing};
use tokio::runtime::Handle;
use tokio::sync::oneshot::{self, error::TryRecvError};
use winit::event_loop::EventLoopProxy;

use crate::app::UserEvent;
use crate::clipsync::{ClipBridge, ListResult, Request};
use crate::theme;

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
            .default_size([480.0, 440.0])
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 10.0);

                // Top navigation bar
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
                    if theme::plain_button(ui, "Home").clicked() {
                        go = Some(DirTarget::Home);
                    }
                    let here = self.listing.as_ref().map(|l| l.path.clone());
                    let has_parent = here.as_deref().is_some_and(|p| !p.is_empty());
                    ui.add_enabled_ui(has_parent, |ui| {
                        if theme::plain_button(ui, "Up").clicked() {
                            go = here.clone().map(DirTarget::Parent);
                        }
                    });
                    if theme::plain_button(ui, "Drives").clicked() {
                        go = Some(DirTarget::Roots);
                    }
                    ui.add_space(8.0);
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

                // Path header
                if let Some(l) = &self.listing {
                    let path_text = if l.path.is_empty() {
                        "Drives"
                    } else {
                        l.path.as_str()
                    };
                    ui.label(
                        egui::RichText::new(path_text)
                            .font(egui::FontId::new(12.0, theme::bold_family()))
                            .color(theme::MUTED),
                    );
                }
                if let Some(e) = &self.error {
                    ui.colored_label(theme::ERR, e);
                }

                let Some(l) = &self.listing else {
                    return;
                };

                let card = theme::panel_card();
                card.show(ui, |ui| {
                    let filter = self.filter.to_lowercase();
                    egui::ScrollArea::vertical()
                        .auto_shrink(false)
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(0.0, 4.0);
                            for e in l.entries.iter().filter(|e| {
                                (self.show_hidden || !e.name.starts_with('.'))
                                    && (filter.is_empty()
                                        || e.name.to_lowercase().contains(&filter))
                            }) {
                                ui.horizontal(|ui| {
                                    if e.dir {
                                        let dir_name = format!("{}/", e.name);
                                        if ui
                                            .link(
                                                egui::RichText::new(&dir_name)
                                                    .font(egui::FontId::new(
                                                        13.0,
                                                        theme::bold_family(),
                                                    ))
                                                    .color(theme::ACCENT),
                                            )
                                            .clicked()
                                        {
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
                                        if ui.add(theme::ghost("Download")).clicked() {
                                            fetch = Some((l.path.clone(), e.name.clone()));
                                        }
                                        ui.label(
                                            egui::RichText::new(&e.name)
                                                .font(egui::FontId::new(
                                                    13.0,
                                                    egui::FontFamily::Proportional,
                                                ))
                                                .color(theme::TEXT),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.label(
                                                    theme::mono(size(e.size), 11.0)
                                                        .color(theme::MUTED),
                                                );
                                            },
                                        );
                                    }
                                });
                            }
                            if l.truncated {
                                ui.label(
                                    egui::RichText::new(
                                        "More entries than fit: use the filter or open a subfolder",
                                    )
                                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                                    .color(theme::MUTED),
                                );
                            }
                        });
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
