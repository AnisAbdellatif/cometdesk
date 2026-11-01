//! Central device list, empty state, and per-device action cards.
//!
//! Entry point: `DevicesUi::content`. Read `reachability.rs` for status dot probing.

use egui::{
    Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2,
};

use super::model::{DevicesUi, Form};
use crate::theme;

/// Helper for menu rows with an icon and label.
fn menu_item(
    ui: &mut egui::Ui,
    icon: theme::Icon,
    text: &str,
    text_color: Color32,
) -> egui::Response {
    let desired_size = Vec2::new(ui.available_width().max(140.0), 34.0);
    let (rect, resp) = ui.allocate_exact_size(desired_size, Sense::click());
    if ui.is_rect_visible(rect) {
        if resp.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), theme::BORDER);
        }
        let icon_rect = Rect::from_center_size(
            Pos2::new(rect.min.x + 18.0, rect.center().y),
            Vec2::splat(14.0),
        );
        theme::draw_icon(ui.painter(), icon_rect, icon, text_color);
        ui.painter().text(
            Pos2::new(rect.min.x + 36.0, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            FontId::new(13.5, FontFamily::Proportional),
            text_color,
        );
    }
    resp
}

/// Card action chosen on a single device tile.
enum CardAction {
    None,
    Connect,
    Edit,
    Remove,
}

impl DevicesUi {
    pub(super) fn content(
        &mut self,
        ui: &mut egui::Ui,
        busy: Option<&str>,
        side: f32,
        body_w: f32,
    ) {
        let busy = if self.looking_up.is_some() {
            Some("Checking the saved password…")
        } else {
            busy
        };

        self.probe_hosts(ui.ctx());

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(side);
                ui.vertical(|ui| {
                    ui.set_width(body_w);
                    ui.add_space(32.0);

                    if let Some(status) = busy {
                        theme::panel_card().show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(egui::RichText::new(status).color(theme::TEXT));
                            });
                        });
                        ui.add_space(16.0);
                    }

                    if let Some(e) = &self.error {
                        theme::banner(ui, theme::ERR, e);
                        ui.add_space(16.0);
                    }

                    if self.store.devices.is_empty() {
                        self.empty_state(ui, body_w);
                    } else {
                        self.device_grid(ui, busy, body_w);
                    }

                    ui.add_space(32.0);
                });
                ui.add_space(side);
            });
        });
    }

    fn empty_state(&mut self, ui: &mut egui::Ui, body_w: f32) {
        // Empty state: centered card
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            let card_w = body_w.min(460.0);
            ui.set_width(card_w);
            theme::panel_card()
                .inner_margin(egui::Margin::same(24))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(theme::bold("Add your Comet", 20.0).color(theme::TEXT));
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(
                                "Enter its address and password. Over USB or \
                                 its own Wi-Fi it is usually ",
                            )
                            .size(13.5)
                            .color(theme::MUTED),
                        );
                        ui.label(theme::mono("192.168.200.1", 13.5).color(theme::TEXT));
                        ui.label(egui::RichText::new(".").size(13.5).color(theme::MUTED));
                    });
                    ui.add_space(16.0);
                    if ui
                        .add(theme::primary("Add device").min_size(Vec2::new(120.0, 36.0)))
                        .clicked()
                    {
                        self.form = Some(self.blank_form());
                    }
                });
        });
    }

    fn device_grid(&mut self, ui: &mut egui::Ui, busy: Option<&str>, body_w: f32) {
        // Caption row
        ui.horizontal(|ui| {
            ui.label(
                theme::bold(format!("YOUR DEVICES · {}", self.store.devices.len()), 12.5)
                    .color(theme::MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("Reachability is probed every 30 s")
                        .size(12.5)
                        .color(theme::MUTED),
                );
            });
        });
        ui.add_space(20.0);

        let mut connect = None;
        let mut edit = None;
        let mut remove = None;

        // One yellow action per screen: Connect on the first reachable device
        let lead = self
            .store
            .devices
            .iter()
            .position(|d| self.reachability(&d.host) == Some(true))
            .unwrap_or(0);

        let columns = if body_w < 700.0 { 1 } else { 2 };
        let total_items = self.store.devices.len() + 1; // devices + Add tile
        let gap = 20.0;
        let col_w = if columns == 1 {
            body_w
        } else {
            (body_w - gap) / 2.0
        };
        let col_h = 176.0;

        let n_rows = total_items.div_ceil(columns);
        for r in 0..n_rows {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for c in 0..columns {
                    let idx = r * columns + c;
                    if idx >= total_items {
                        break;
                    }
                    if idx < self.store.devices.len() {
                        match self.device_card(ui, idx, idx == lead, busy.is_some(), col_w, col_h) {
                            CardAction::Connect => connect = Some(idx),
                            CardAction::Edit => edit = Some(idx),
                            CardAction::Remove => remove = Some(idx),
                            CardAction::None => {}
                        }
                    } else {
                        self.add_tile(ui, col_w, col_h);
                    }
                }
            });
            ui.add_space(gap);
        }

        if let Some(i) = connect {
            self.connect(self.store.devices[i].clone());
        }
        if let Some(i) = edit {
            self.form = Some(Form {
                editing: Some(i),
                device: self.store.devices[i].clone(),
                password: String::new(),
                remember: true,
            });
        }
        if remove.is_some() {
            self.confirm_remove = remove;
        }
    }

    fn device_card(
        &self,
        ui: &mut egui::Ui,
        i: usize,
        is_lead: bool,
        is_busy: bool,
        col_w: f32,
        col_h: f32,
    ) -> CardAction {
        let mut action = CardAction::None;
        let d = &self.store.devices[i];
        let up = self.reachability(&d.host);
        let card_frame = theme::panel_card().inner_margin(egui::Margin::symmetric(24, 20));
        card_frame.show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(col_w - 48.0);
                ui.set_min_height(col_h - 40.0);
                // Top row: badge, name + mono address, reachability
                ui.horizontal(|ui| {
                    theme::badge(ui, &d.host, d.display_name(), 48.0);
                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        ui.label(theme::bold(d.display_name(), 18.0).color(theme::TEXT));
                        ui.label(
                            theme::mono(
                                format!("{}@{} · {}", d.user, d.host, d.target.label()),
                                12.5,
                            )
                            .color(theme::MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            match up {
                                Some(true) => theme::status_dot(ui, theme::OK, "Reachable"),
                                Some(false) => theme::status_dot(ui, theme::WARN, "Not reachable"),
                                None => theme::status_dot(ui, theme::MUTED, "Checking…"),
                            }
                        });
                    });
                });

                ui.add_space((ui.available_height() - 36.0).max(20.0));

                // Second row: Connect + More button
                ui.horizontal(|ui| {
                    let spacing = 10.0;
                    let more_size = 36.0;
                    let connect_width = (ui.available_width() - more_size - spacing).max(60.0);

                    let button = if is_lead {
                        theme::primary("Connect").min_size(Vec2::new(connect_width, 36.0))
                    } else {
                        egui::Button::new(
                            egui::RichText::new("Connect")
                                .font(FontId::new(14.0, theme::bold_family()))
                                .color(theme::TEXT),
                        )
                        .fill(theme::CARD_HOVER)
                        .stroke(Stroke::new(1.0, theme::BORDER))
                        .corner_radius(CornerRadius::same(6))
                        .min_size(Vec2::new(connect_width, 36.0))
                    };

                    if ui.add_enabled(!is_busy, button).clicked() {
                        action = CardAction::Connect;
                    }

                    let (more_rect, more_resp) =
                        ui.allocate_exact_size(Vec2::splat(more_size), Sense::click());
                    if ui.is_rect_visible(more_rect) {
                        let is_open = egui::Popup::is_id_open(
                            ui.ctx(),
                            egui::Popup::default_response_id(&more_resp),
                        );
                        let highlighted = more_resp.hovered() || is_open;
                        let bg = theme::CARD_HOVER;
                        let border = if highlighted {
                            theme::TEXT
                        } else {
                            theme::BORDER
                        };
                        ui.painter().rect(
                            more_rect,
                            CornerRadius::same(6),
                            bg,
                            Stroke::new(1.0, border),
                            StrokeKind::Inside,
                        );
                        theme::draw_icon(
                            ui.painter(),
                            more_rect,
                            theme::Icon::More,
                            if highlighted {
                                theme::TEXT
                            } else {
                                theme::MUTED
                            },
                        );
                    }

                    let popup_frame = egui::Frame::NONE
                        .fill(theme::CARD_HOVER)
                        .stroke(Stroke::new(1.0, theme::BORDER))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(egui::Margin::same(6));
                    egui::Popup::menu(&more_resp).frame(popup_frame).show(|ui| {
                        ui.set_min_width(140.0);
                        if menu_item(ui, theme::Icon::Pencil, "Edit…", theme::TEXT).clicked() {
                            action = CardAction::Edit;
                            ui.close();
                        }
                        if menu_item(ui, theme::Icon::Trash, "Remove…", theme::ERR).clicked() {
                            action = CardAction::Remove;
                            ui.close();
                        }
                    });
                });
            });
        });
        action
    }

    fn add_tile(&mut self, ui: &mut egui::Ui, col_w: f32, col_h: f32) {
        // "Add another Comet" dashed tile
        let (add_tile_rect, add_tile_resp) =
            ui.allocate_exact_size(Vec2::new(col_w, col_h), Sense::click());
        if ui.is_rect_visible(add_tile_rect) {
            let hovered = add_tile_resp.hovered();
            let bg = if hovered {
                theme::CARD_HOVER
            } else {
                Color32::TRANSPARENT
            };
            let border_color = if hovered {
                theme::ACCENT
            } else {
                theme::BORDER
            };
            if hovered {
                ui.painter()
                    .rect_filled(add_tile_rect, CornerRadius::same(10), bg);
            }
            theme::draw_dashed_rect(
                ui.painter(),
                add_tile_rect,
                10.0,
                Stroke::new(1.5, border_color),
                7.0,
                6.0,
            );
            let center = add_tile_rect.center();
            let plus_center = Pos2::new(center.x, center.y - 14.0);
            let plus_rect = Rect::from_center_size(plus_center, Vec2::splat(20.0));
            theme::draw_icon(ui.painter(), plus_rect, theme::Icon::Plus, theme::ACCENT);
            ui.painter().text(
                Pos2::new(center.x, center.y + 16.0),
                Align2::CENTER_CENTER,
                "Add another Comet",
                FontId::new(14.5, theme::bold_family()),
                if hovered { theme::TEXT } else { theme::MUTED },
            );
        }
        if add_tile_resp.clicked() {
            self.form = Some(self.blank_form());
        }
    }
}
