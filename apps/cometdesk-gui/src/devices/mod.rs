//! Start screen: saved devices, add/edit form, password and 2FA prompts.
//!
//! Entry point: `DevicesUi::ui`. Read `list.rs` for the device grid and `model.rs` for state.

mod form_modal;
mod list;
mod model;
mod prompt;
mod reachability;
mod remove_dialog;

pub use model::{ConnectError, ConnectRequest, DevicesUi};

use egui::{Align2, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::theme;

impl DevicesUi {
    /// Draws the screen. Returns a connect request when the user starts one.
    pub fn ui(&mut self, ui: &mut egui::Ui, busy: Option<&str>) -> Option<ConnectRequest> {
        let mut request = None;
        let logo = self
            .logo
            .get_or_insert_with(|| theme::logo_texture(ui.ctx()).expect("embedded logo decodes"))
            .clone();

        let avail_w = ui.available_width();
        let max_w = 1168.0_f32;
        let side = ((avail_w - max_w) / 2.0).max(56.0);
        let body_w = (avail_w - 2.0 * side).min(max_w);

        // 1. Top header panel
        let header_frame = egui::Frame::NONE
            .fill(theme::PANEL)
            .stroke(Stroke::new(1.0, theme::BORDER))
            .inner_margin(egui::Margin::symmetric(40, 16));
        egui::Panel::top("connect_header")
            .frame(header_frame)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(side);
                    ui.set_width(body_w);
                    ui.add(egui::Image::new(&logo).fit_to_exact_size(Vec2::splat(40.0)));
                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(theme::bold("CometDesk", 20.0).color(theme::TEXT));
                        ui.label(
                            egui::RichText::new("Remote console for GL.iNet KVM")
                                .size(13.0)
                                .color(theme::MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let add_btn_size = Vec2::new(124.0, 38.0);
                        let (rect, resp) = ui.allocate_exact_size(add_btn_size, Sense::click());
                        if ui.is_rect_visible(rect) {
                            let hovered = resp.hovered();
                            let bg = if hovered {
                                theme::CARD_HOVER
                            } else {
                                theme::CARD
                            };
                            let border = if hovered {
                                theme::ACCENT
                            } else {
                                theme::BORDER
                            };
                            ui.painter().rect(
                                rect,
                                CornerRadius::same(8),
                                bg,
                                Stroke::new(1.0, border),
                                StrokeKind::Inside,
                            );
                            let icon_rect = Rect::from_center_size(
                                Pos2::new(rect.min.x + 22.0, rect.center().y),
                                Vec2::splat(14.0),
                            );
                            theme::draw_icon(
                                ui.painter(),
                                icon_rect,
                                theme::Icon::Plus,
                                theme::ACCENT,
                            );
                            ui.painter().text(
                                Pos2::new(rect.min.x + 38.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                "Add device",
                                FontId::new(14.0, theme::bold_family()),
                                theme::TEXT,
                            );
                        }
                        if resp.clicked() {
                            self.form = Some(self.blank_form());
                        }
                    });
                    ui.add_space(side);
                });
            });

        // 2. Bottom footer panel
        let footer_frame = egui::Frame::NONE
            .fill(theme::PANEL)
            .stroke(Stroke::new(1.0, theme::BORDER))
            .inner_margin(egui::Margin::symmetric(40, 16));
        egui::Panel::bottom("connect_footer")
            .frame(footer_frame)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(side);
                    ui.set_width(body_w);
                    ui.label(
                        egui::RichText::new("First time? A new Comet answers at ")
                            .size(13.0)
                            .color(theme::MUTED),
                    );
                    ui.label(theme::mono("192.168.200.1", 13.0).color(theme::TEXT));
                    ui.label(
                        egui::RichText::new(" over USB or its own Wi-Fi.")
                            .size(13.0)
                            .color(theme::MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let version = format!("v{}", env!("CARGO_PKG_VERSION"));
                        ui.label(egui::RichText::new(version).size(13.0).color(theme::MUTED));
                        ui.add_space(20.0);
                        ui.label(
                            egui::RichText::new("Passwords stay in the macOS keychain")
                                .size(13.0)
                                .color(theme::MUTED),
                        );
                    });
                    ui.add_space(side);
                });
            });

        // 3. Central body content
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::BG))
            .show(ui, |ui| {
                self.content(ui, busy, side, body_w);
            });

        // 4. Modals on top
        if let Some(r) = self.form_modal(ui.ctx()) {
            request = Some(r);
        }
        if let Some(r) = self.prompt_ui(ui.ctx()) {
            request = Some(r);
        }
        self.remove_dialog(ui.ctx());

        request
    }
}
