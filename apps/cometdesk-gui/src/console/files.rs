//! Sending files to the target through the agent and rendering transfer progress.
//! Entry points: `Console::send_files`, `Console::pick_files_to_send`, `Console::poll_transfers`.

use std::sync::atomic::Ordering;

use super::Console;
use crate::app::UserEvent;
use crate::theme;

impl Console {
    /// Streams `paths` to the target's Downloads folder through the agent.
    pub(super) fn send_files(&mut self, paths: Vec<std::path::PathBuf>) {
        let Some(clip) = self.clip.as_ref() else {
            return;
        };
        let mut errors = Vec::new();
        for path in paths {
            match clip.send_file(path.clone()) {
                Ok(t) => self.transfers.push(t),
                Err(e) => errors.push(format!("Cannot send {}: {e}", path.display())),
            }
        }
        for e in errors {
            self.notify(e);
        }
    }

    pub(super) fn pick_files_to_send(&self) {
        let (picked, proxy) = (self.picked_files.clone(), self.proxy.clone());
        self.rt.spawn(async move {
            let files = rfd::AsyncFileDialog::new()
                .set_title("Send files to the target")
                .pick_files()
                .await;
            if let Some(files) = files {
                picked
                    .lock()
                    .unwrap()
                    .extend(files.iter().map(|f| f.path().to_owned()));
            }
            let _ = proxy.send_event(UserEvent::Refresh);
        });
    }

    /// Reports finished transfers and draws the running ones.
    pub(super) fn poll_transfers(&mut self, ctx: &egui::Context) {
        use tokio::sync::oneshot::error::TryRecvError;
        let mut notes = Vec::new();
        self.transfers.retain_mut(|t| match t.result.try_recv() {
            Ok(Ok(path)) => {
                notes.push(format!("Sent {} to the target: {path}", t.progress.name));
                false
            }
            Ok(Err(e)) => {
                notes.push(format!("Sending {} failed: {e}", t.progress.name));
                false
            }
            Err(TryRecvError::Closed) => {
                notes.push(format!("Sending {} was interrupted", t.progress.name));
                false
            }
            Err(TryRecvError::Empty) => true,
        });
        for n in notes {
            self.notify(n);
        }
        if self.transfers.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::unique("transfers"))
            .anchor(egui::Align2::LEFT_BOTTOM, [8.0, -8.0])
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(theme::PANEL)
                    .stroke(egui::Stroke::new(1.0, theme::BORDER))
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        for p in self.transfers.iter().map(|t| &t.progress) {
                            let sent = p.sent.load(Ordering::Relaxed);
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::ProgressBar::new(sent as f32 / p.total.max(1) as f32)
                                        .desired_width(240.0)
                                        .text(format!(
                                            "{} · {:.1} / {:.1} MB",
                                            p.name,
                                            sent as f64 / 1e6,
                                            p.total as f64 / 1e6
                                        )),
                                );
                                if ui.button("Cancel").clicked() {
                                    p.cancel.store(true, Ordering::Relaxed);
                                }
                            });
                        }
                    });
            });
    }
}
