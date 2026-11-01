//! OS-level session actions card (lock, sleep, log off, restart, shut down) and target notifications.
//! Entry points: `Console::session_menu`, `Console::target_note`.

use cometdesk_clip::PowerAction as SessionAction;

use crate::console::state::Confirm;
use crate::console::{Console, debug_view};
use crate::theme;

impl Console {
    /// Lock, sleep, log off, restart and shut down through the agent: the OS closes the
    /// session cleanly, unlike the ATX button. Shown only while the agent is connected.
    pub(in crate::console) fn session_menu(&mut self, ui: &mut egui::Ui) {
        if !self.inserts_via_agent() && debug_view().as_deref() != Some("panel-target") {
            return;
        }

        let card_frame = egui::Frame::NONE
            .fill(theme::CARD)
            .stroke(egui::Stroke::new(1.0, theme::BORDER))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin {
                left: 14,
                right: 14,
                top: 12,
                bottom: 12,
            });

        card_frame.show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 10.0);
            ui.label(
                egui::RichText::new("Session")
                    .font(egui::FontId::new(14.0, theme::bold_family()))
                    .color(theme::TEXT),
            );

            let btn_w = ((ui.available_width() - 8.0) * 0.5).floor();

            // Row 1: Lock, Sleep
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.allocate_ui(egui::vec2(btn_w, 34.0), |ui| {
                    if theme::secondary_button(ui, "Lock", 34.0, false).clicked() {
                        self.session_action(SessionAction::Lock);
                    }
                });
                ui.allocate_ui(egui::vec2(btn_w, 34.0), |ui| {
                    if theme::secondary_button(ui, "Sleep", 34.0, false).clicked() {
                        self.session_action(SessionAction::Sleep);
                    }
                });
            });

            // Row 2: Log off…, Restart…
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.allocate_ui(egui::vec2(btn_w, 34.0), |ui| {
                    if theme::secondary_button(ui, "Log off…", 34.0, false).clicked() {
                        self.confirm = Some(Confirm::Session(SessionAction::Logoff));
                    }
                });
                ui.allocate_ui(egui::vec2(btn_w, 34.0), |ui| {
                    if theme::secondary_button(ui, "Restart…", 34.0, false).clicked() {
                        self.confirm = Some(Confirm::Session(SessionAction::Restart));
                    }
                });
            });

            // Shut down… as full-width row
            if theme::secondary_button(ui, "Shut down…", 34.0, false).clicked() {
                self.confirm = Some(Confirm::Session(SessionAction::Shutdown));
            }
        });
    }

    /// A message from the target: a toast while the window is in front, else a system
    /// notification (a toast if the platform has none).
    pub(in crate::console) fn target_note(&mut self, title: &str, body: &str) {
        let host = self.device.host.clone();
        if !self.window_focused && crate::sysnotify::show(title, body, &host) {
            return;
        }
        self.notify(if body.is_empty() {
            title.to_owned()
        } else {
            format!("{title}: {body}")
        });
    }
}
