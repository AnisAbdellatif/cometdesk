//! Add/edit saved device modal dialog.
//!
//! Entry point: `DevicesUi::form_modal`. Read `model.rs` for form state and store persistence.

use egui::{Color32, CornerRadius, FontFamily, FontId, Sense, Stroke, Vec2};

use super::{ConnectRequest, DevicesUi};
use crate::store::TargetOs;
use crate::theme;

impl DevicesUi {
    /// Add/Edit device form shown as a centered modal dialog.
    pub(super) fn form_modal(&mut self, ctx: &egui::Context) -> Option<ConnectRequest> {
        let form = self.form.as_mut()?;
        let mut save = false;
        let mut cancel = false;
        let is_editing = form.editing.is_some();

        egui::Modal::new(egui::Id::unique("device-form"))
            .frame(theme::modal_frame())
            .backdrop_color(Color32::from_black_alpha(180))
            .show(ctx, |ui| {
                ui.set_width(440.0);
                ui.label(
                    theme::bold(
                        if is_editing {
                            "Edit device"
                        } else {
                            "Add device"
                        },
                        18.0,
                    )
                    .color(theme::TEXT),
                );

                if !is_editing {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "A new Comet answers at 192.168.200.1 over USB or its own Wi-Fi.",
                        )
                        .size(13.0)
                        .color(theme::MUTED),
                    );
                }

                ui.add_space(16.0);

                // Name
                theme::field_label(ui, "Name");
                ui.add(
                    egui::TextEdit::singleline(&mut form.device.name)
                        .hint_text("Office Comet")
                        .desired_width(f32::INFINITY)
                        .min_size(Vec2::new(0.0, 36.0)),
                );

                ui.add_space(12.0);

                // Address & User side-by-side (Address wider)
                ui.horizontal(|ui| {
                    let gap = 12.0;
                    let user_w = 130.0;
                    let addr_w = (ui.available_width() - user_w - gap).max(100.0);

                    ui.vertical(|ui| {
                        ui.set_width(addr_w);
                        theme::field_label(ui, "Address");
                        ui.add(
                            egui::TextEdit::singleline(&mut form.device.host)
                                .hint_text("192.168.200.1 or host:port")
                                .font(FontId::new(13.0, FontFamily::Monospace))
                                .desired_width(f32::INFINITY)
                                .min_size(Vec2::new(0.0, 36.0)),
                        );
                    });
                    ui.add_space(gap);
                    ui.vertical(|ui| {
                        ui.set_width(user_w);
                        theme::field_label(ui, "User");
                        ui.add(
                            egui::TextEdit::singleline(&mut form.device.user)
                                .hint_text("admin")
                                .desired_width(f32::INFINITY)
                                .min_size(Vec2::new(0.0, 36.0)),
                        );
                    });
                });

                ui.add_space(12.0);

                // Password
                theme::field_label(ui, "Password");
                ui.add(
                    egui::TextEdit::singleline(&mut form.password)
                        .password(true)
                        .hint_text(if is_editing { "unchanged" } else { "" })
                        .desired_width(f32::INFINITY)
                        .min_size(Vec2::new(0.0, 36.0)),
                );

                ui.add_space(12.0);

                // Target runs
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label(
                        egui::RichText::new("Target runs")
                            .font(FontId::new(12.0, theme::bold_family()))
                            .color(theme::MUTED),
                    );
                    ui.label(
                        egui::RichText::new(" · sets shortcuts and paste behaviour")
                            .size(12.0)
                            .color(theme::MUTED),
                    );
                });
                ui.add_space(6.0);
                let mut seg_idx = match form.device.target {
                    TargetOs::Linux => 0,
                    TargetOs::Windows => 1,
                    TargetOs::MacOs => 2,
                    TargetOs::Bios => 3,
                };
                theme::segmented(
                    ui,
                    &mut seg_idx,
                    &["Linux", "Windows", "macOS", "BIOS / other"],
                );
                form.device.target = match seg_idx {
                    0 => TargetOs::Linux,
                    1 => TargetOs::Windows,
                    2 => TargetOs::MacOs,
                    _ => TargetOs::Bios,
                };

                ui.add_space(14.0);

                // Keychain checkbox
                ui.checkbox(&mut form.remember, "Remember password in the keychain");

                ui.add_space(16.0);

                // 1px rule
                let (rule_rect, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                ui.painter().hline(
                    rule_rect.x_range(),
                    rule_rect.center().y,
                    Stroke::new(1.0, theme::BORDER),
                );

                ui.add_space(16.0);

                // Actions: Cancel + Save and connect
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let valid = !form.device.host.trim().is_empty()
                            && !form.device.user.trim().is_empty();
                        save = ui
                            .add_enabled(
                                valid,
                                theme::primary("Save and connect").min_size(Vec2::new(150.0, 36.0)),
                            )
                            .clicked();
                        cancel = ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Cancel")
                                        .font(FontId::new(14.0, theme::bold_family()))
                                        .color(theme::TEXT),
                                )
                                .fill(theme::CARD_HOVER)
                                .stroke(Stroke::new(1.0, theme::BORDER))
                                .corner_radius(CornerRadius::same(6))
                                .min_size(Vec2::new(90.0, 36.0)),
                            )
                            .clicked();
                    });
                });
            });

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        let valid = !form.device.host.trim().is_empty() && !form.device.user.trim().is_empty();
        if valid && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            save = true;
        }

        if cancel {
            self.form = None;
            return None;
        }
        if !save {
            return None;
        }

        let mut form = self.form.take()?;
        form.device.host = cometdesk::client::normalize_host(&form.device.host);
        form.device.user = form.device.user.trim().to_owned();
        if let Some(i) = form.editing {
            if let Some(old) = self.store.devices.get(i)
                && (old.host != form.device.host || old.user != form.device.user)
            {
                self.forget_password(old.clone());
            }
            self.store.remove(i);
        }
        self.store.upsert(form.device.clone());
        if !form.password.is_empty() && form.remember {
            self.save_password(form.device.clone(), form.password.clone());
        }
        self.save();
        if form.password.is_empty() {
            self.connect(form.device);
            None
        } else {
            Some(ConnectRequest {
                device: form.device,
                password: form.password,
                totp: None,
            })
        }
    }
}
