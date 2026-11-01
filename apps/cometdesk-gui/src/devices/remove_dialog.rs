//! Confirmation modal before deleting a saved device and its keychain password.
//!
//! Entry point: `DevicesUi::remove_dialog`.

use egui::{Color32, CornerRadius, FontId, Sense, Stroke, Vec2};

use super::DevicesUi;
use crate::theme;

impl DevicesUi {
    /// Asks before deleting a saved device: its keychain password goes with it.
    pub(super) fn remove_dialog(&mut self, ctx: &egui::Context) {
        let Some(i) = self.confirm_remove else { return };
        let Some(device) = self.store.devices.get(i) else {
            self.confirm_remove = None;
            return;
        };
        let name = device.display_name();
        let (mut confirm, mut cancel) = (false, false);
        egui::Modal::new(egui::Id::unique("remove-device"))
            .frame(theme::modal_frame())
            .backdrop_color(Color32::from_black_alpha(180))
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(theme::bold(format!("Remove {name}?"), 18.0).color(theme::TEXT));
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "The device and its saved password are deleted from this Mac.",
                    )
                    .size(13.0)
                    .color(theme::MUTED),
                );
                ui.add_space(16.0);
                let (rule_rect, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                ui.painter().hline(
                    rule_rect.x_range(),
                    rule_rect.center().y,
                    Stroke::new(1.0, theme::BORDER),
                );
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        confirm = ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Remove")
                                        .font(FontId::new(14.0, theme::bold_family()))
                                        .color(theme::BG),
                                )
                                .fill(theme::ERR)
                                .corner_radius(CornerRadius::same(6))
                                .min_size(Vec2::new(90.0, 36.0)),
                            )
                            .clicked();
                        cancel = ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Keep")
                                        .font(FontId::new(14.0, theme::bold_family()))
                                        .color(theme::TEXT),
                                )
                                .fill(theme::CARD_HOVER)
                                .stroke(Stroke::new(1.0, theme::BORDER))
                                .corner_radius(CornerRadius::same(6))
                                .min_size(Vec2::new(80.0, 36.0)),
                            )
                            .clicked();
                    });
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if confirm && let Some(d) = self.store.remove(i) {
            self.forget_password(d);
            self.save();
        }
        if confirm || cancel {
            self.confirm_remove = None;
        }
    }
}
