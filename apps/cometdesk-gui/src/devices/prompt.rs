//! Password and TOTP 2FA modal prompts during connection.
//!
//! Entry point: `DevicesUi::prompt_ui`. Read `model.rs` for `Prompt` variants and error routing.

use egui::{Color32, CornerRadius, FontFamily, FontId, Sense, Stroke, Vec2};

use super::model::Prompt;
use super::{ConnectRequest, DevicesUi};
use crate::store::SavedDevice;
use crate::theme;

impl DevicesUi {
    pub(super) fn prompt_ui(&mut self, ctx: &egui::Context) -> Option<ConnectRequest> {
        let prompt = self.prompt.as_mut()?;
        let mut submit = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("prompt"))
            .frame(theme::modal_frame())
            .backdrop_color(Color32::from_black_alpha(180))
            .show(ctx, |ui| {
                ui.set_width(380.0);
                match prompt {
                    Prompt::Password {
                        device,
                        password,
                        remember,
                        wrong,
                    } => {
                        (submit, cancel) =
                            Self::password_prompt(ctx, ui, device, password, remember, *wrong);
                    }
                    Prompt::Totp { device, code, .. } => {
                        (submit, cancel) = Self::totp_prompt(ctx, ui, device, code);
                    }
                }
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if cancel {
            self.prompt = None;
            return None;
        }
        if !submit {
            return None;
        }
        match self.prompt.take()? {
            Prompt::Password {
                device,
                password,
                remember,
                ..
            } => {
                if remember {
                    self.save_password(device.clone(), password.clone());
                }
                Some(ConnectRequest {
                    device,
                    password,
                    totp: None,
                })
            }
            Prompt::Totp {
                device,
                password,
                code,
            } => Some(ConnectRequest {
                device,
                password,
                totp: Some(code.trim().to_owned()),
            }),
        }
    }

    fn password_prompt(
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        device: &SavedDevice,
        password: &mut String,
        remember: &mut bool,
        wrong: bool,
    ) -> (bool, bool) {
        let mut submit;
        let mut cancel = false;
        ui.label(
            theme::bold(
                format!("Password for {}@{}", device.user, device.host),
                18.0,
            )
            .color(theme::TEXT),
        );
        if wrong {
            ui.add_space(8.0);
            theme::banner(ui, theme::ERR, "The saved password was rejected.");
        }
        ui.add_space(12.0);
        theme::field_label(ui, "Password");
        let r = ui.add(
            egui::TextEdit::singleline(password)
                .password(true)
                .desired_width(f32::INFINITY)
                .min_size(Vec2::new(0.0, 36.0)),
        );
        r.request_focus();
        submit = r.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(12.0);
        ui.checkbox(remember, "Remember in the keychain");
        ui.add_space(14.0);
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
                submit |= ui
                    .add(theme::primary("Connect").min_size(Vec2::new(100.0, 36.0)))
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
                        .min_size(Vec2::new(80.0, 36.0)),
                    )
                    .clicked();
            });
        });
        (submit, cancel)
    }

    fn totp_prompt(
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        device: &SavedDevice,
        code: &mut String,
    ) -> (bool, bool) {
        let mut submit;
        let mut cancel = false;
        ui.label(
            theme::bold(format!("2FA code for {}", device.display_name()), 18.0).color(theme::TEXT),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("The device has two-factor authentication enabled.")
                .size(13.0)
                .color(theme::MUTED),
        );
        ui.add_space(12.0);
        theme::field_label(ui, "Code");
        let r = ui.add(
            egui::TextEdit::singleline(code)
                .char_limit(6)
                .hint_text("123456")
                .font(FontId::new(14.0, FontFamily::Monospace))
                .desired_width(f32::INFINITY)
                .min_size(Vec2::new(0.0, 36.0)),
        );
        r.request_focus();
        submit = r.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(14.0);
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
                submit |= ui
                    .add(theme::primary("Connect").min_size(Vec2::new(100.0, 36.0)))
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
                        .min_size(Vec2::new(80.0, 36.0)),
                    )
                    .clicked();
            });
        });
        (submit, cancel)
    }
}
