//! Video canvas overlays: signal messages, region selection, fullscreen status, stats, typing progress and toasts.
//! Entry point: `Console::overlays`.

use std::sync::atomic::Ordering;

use crate::console::Console;
use crate::theme;

impl Console {
    pub(super) fn overlays(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let (status, _) = self.status();
        let center_msg = match status.as_str() {
            "No signal" => Some("No signal: the target is off or asleep"),
            "Reconnecting…" => Some("Reconnecting…"),
            "Paused (window hidden)" => Some("Resuming…"),
            _ if self.stats.window_start.is_none() && self.stats.shown.is_empty() => {
                Some("Waiting for video…")
            }
            _ => None,
        };
        if let Some(msg) = center_msg {
            ui.put(
                rect,
                egui::Label::new(egui::RichText::new(msg).size(18.0).color(theme::MUTED)),
            );
        }
        self.region_overlay(ui);
        // Fullscreen hides the toolbar and its status: surface anything that is not OK.
        let (text, color) = self.status();
        if self.fullscreen && self.toolbar_rect.is_none() && color != theme::OK {
            egui::Area::new(egui::Id::unique("fullscreen-status"))
                .anchor(egui::Align2::CENTER_TOP, [0.0, 12.0])
                .interactable(false)
                .show(ui.ctx(), |ui| theme::floating_status(ui, color, &text));
        }
        if self.show_stats && !self.stats.shown.is_empty() {
            egui::Area::new(egui::Id::unique("stats"))
                .anchor(egui::Align2::RIGHT_BOTTOM, [-8.0, -8.0])
                .show(ui.ctx(), |ui| {
                    egui::Frame::NONE
                        .fill(theme::PANEL)
                        .stroke(egui::Stroke::new(1.0, theme::BORDER))
                        .corner_radius(egui::CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(&self.stats.shown)
                                    .font(egui::FontId::new(12.0, egui::FontFamily::Monospace))
                                    .color(theme::TEXT),
                            );
                        });
                });
        }
        if let Some(typing) = &self.typing {
            let done = typing.done.load(Ordering::Relaxed);
            let mut cancel = false;
            egui::Area::new(egui::Id::unique("typing"))
                .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -16.0])
                .show(ui.ctx(), |ui| {
                    egui::Frame::NONE
                        .fill(theme::PANEL)
                        .stroke(egui::Stroke::new(1.0, theme::BORDER))
                        .corner_radius(egui::CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(12, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::ProgressBar::new(
                                        done as f32 / typing.total.max(1) as f32,
                                    )
                                    .desired_width(220.0)
                                    .text(format!("Typing {done}/{}", typing.total)),
                                );
                                cancel = ui.button("Cancel").clicked();
                            });
                        });
                });
            if cancel {
                self.cancel_typing();
            } else if typing.task.is_finished() {
                let task = self.typing.take().expect("checked above").task;
                let result = self.rt.block_on(task);
                match result {
                    Ok(Ok(())) => self.notify("Typed the clipboard".into()),
                    Ok(Err(e)) => self.notify(format!("Typing failed: {e}")),
                    Err(_) => {}
                }
            }
        }
        if let Some((text, _)) = &self.notice {
            egui::Area::new(egui::Id::unique("notice"))
                .anchor(egui::Align2::CENTER_TOP, [0.0, 48.0])
                .show(ui.ctx(), |ui| {
                    egui::Frame::NONE
                        .fill(theme::PANEL)
                        .stroke(egui::Stroke::new(1.0, theme::BORDER))
                        .corner_radius(egui::CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(16, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(8.0, 8.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().circle_filled(rect.center(), 4.0, theme::OK);
                                ui.label(
                                    egui::RichText::new(text)
                                        .font(egui::FontId::new(
                                            13.0,
                                            egui::FontFamily::Proportional,
                                        ))
                                        .color(theme::TEXT),
                                );
                            });
                        });
                });
        }
    }
}
