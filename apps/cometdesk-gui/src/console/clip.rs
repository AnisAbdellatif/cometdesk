//! Clipboard bridge lifecycle, pairing management and the Clipboard card in the Target tab.
//! Entry points: `Console::on_clip_secret`, `Console::start_clip`, `Console::clip_menu`.

use cometdesk::api::bridge::Health;
use cometdesk_clip::PastePref;

use super::state::Confirm;
use super::{Console, ConsoleAction, debug_view};
use crate::app::UserEvent;
use crate::clipsync::{BridgeState, ClipBridge, Settings as ClipSettings};
use crate::store::{self, ClipDirection, TargetOs};
use crate::theme;

impl Console {
    /// The stored pairing secret finished loading: start the clipboard bridge with it.
    pub fn on_clip_secret(&mut self, secret: cometdesk_clip::Secret) {
        if self.clip.is_none() {
            self.start_clip(secret);
        }
    }

    // ----- clipboard bridge ----------------------------------------------------------------

    fn clip_settings(&self) -> ClipSettings {
        ClipSettings {
            direction: self.device.clip_direction,
            paused: self.clip_paused,
        }
    }

    pub(super) fn start_clip(&mut self, secret: cometdesk_clip::Secret) {
        let proxy = self.proxy.clone();
        self.clip = Some(ClipBridge::spawn(
            &self.rt,
            self.session.clone(),
            secret,
            self.clip_settings(),
            move || {
                let _ = proxy.send_event(UserEvent::Refresh);
            },
        ));
    }

    fn forget_clipboard_pairing(&mut self) {
        let device = self.device.clone();
        self.rt
            .spawn_blocking(move || store::delete_clip_secret(&device));
        self.clip = None;
        self.notify("Clipboard pairing removed".into());
    }

    /// What the Comet side needs, as a button label (`None`: nothing to do).
    pub(super) fn bridge_action(&self) -> Option<&'static str> {
        match self.setup.health()? {
            Health::NotInstalled => Some("Set up this Comet…"),
            Health::Outdated { .. } => Some("Update the Comet's bridge…"),
            Health::Broken(_) => Some("Repair the Comet's bridge…"),
            Health::Ok => None,
        }
    }

    pub(super) fn clip_menu(&mut self, ui: &mut egui::Ui) -> ConsoleAction {
        let mut action = ConsoleAction::None;
        let needs_device =
            self.bridge_action().is_some() || debug_view().as_deref() == Some("panel-target");
        let (state_text, tint) = match self.clip.as_ref().map(ClipBridge::state) {
            _ if debug_view().as_deref() == Some("panel-target") => {
                ("Needs repair", Some(theme::WARN))
            }
            _ if self.setup.progress().is_some() => ("Setting up…", Some(theme::WARN)),
            Some(_) if needs_device => ("Needs repair", Some(theme::WARN)),
            None => ("Off", None),
            Some(_) if self.clip_paused => ("Paused", None),
            Some(BridgeState::Connected(..)) => ("Synced", Some(theme::OK)),
            Some(_) => ("Not connected", Some(theme::WARN)),
        };

        let card_frame = egui::Frame::NONE
            .fill(theme::CARD)
            .stroke(egui::Stroke::new(1.0, theme::BORDER))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::ZERO);

        card_frame.show(ui, |ui| {
            // Header: height 44px
            let (head_rect, head_resp) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::click());
            if ui.is_rect_visible(head_rect) {
                if head_resp.hovered() {
                    ui.painter().rect_filled(
                        head_rect,
                        egui::CornerRadius::same(10),
                        theme::CARD_HOVER,
                    );
                }
                let title_font = egui::FontId::new(14.0, theme::bold_family());
                let title_galley =
                    ui.painter()
                        .layout_no_wrap("Clipboard".to_owned(), title_font, theme::TEXT);
                let title_pos = egui::pos2(
                    head_rect.min.x + 14.0,
                    head_rect.center().y - title_galley.size().y * 0.5,
                );
                let title_w = title_galley.size().x;
                ui.painter().galley(title_pos, title_galley, theme::TEXT);

                if let Some(color) = tint {
                    let bg_color = if color == theme::WARN {
                        egui::Color32::from_rgb(0x2A, 0x1D, 0x0C)
                    } else {
                        color.gamma_multiply(0.18)
                    };
                    let tag_font = egui::FontId::new(12.0, theme::bold_family());
                    let tag_galley =
                        ui.painter()
                            .layout_no_wrap(state_text.to_owned(), tag_font, color);
                    let tag_w = tag_galley.size().x + 16.0;
                    let tag_h = 20.0;
                    let tag_rect = egui::Rect::from_center_size(
                        egui::pos2(
                            head_rect.min.x + 14.0 + title_w + 8.0 + tag_w * 0.5,
                            head_rect.center().y,
                        ),
                        egui::vec2(tag_w, tag_h),
                    );
                    ui.painter()
                        .rect_filled(tag_rect, egui::CornerRadius::same(10), bg_color);
                    ui.painter().galley(
                        egui::pos2(
                            tag_rect.min.x + 8.0,
                            tag_rect.center().y - tag_galley.size().y * 0.5,
                        ),
                        tag_galley,
                        color,
                    );
                }

                let chevron_dir = if self.clip_expanded {
                    theme::ChevronDir::Down
                } else {
                    theme::ChevronDir::Right
                };
                let chevron_pos = egui::pos2(head_rect.max.x - 14.0 - 4.0, head_rect.center().y);
                theme::chevron(ui.painter(), chevron_pos, 4.0, chevron_dir, theme::MUTED);
            }
            if head_resp.clicked() {
                self.clip_expanded = !self.clip_expanded;
            }

            if self.clip_expanded {
                egui::Frame::NONE
                    .inner_margin(egui::Margin {
                        left: 14,
                        right: 14,
                        top: 0,
                        bottom: 14,
                    })
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 12.0);

                        // Comet and Target label/value rows
                        let comet_val = if let Some(p) = self.setup.progress() {
                            p.to_owned()
                        } else {
                            match self.setup.health() {
                                None => "Checking the Comet…".to_string(),
                                Some(Health::Ok) => {
                                    let fw = self
                                        .setup
                                        .status()
                                        .and_then(|s| s.firmware)
                                        .unwrap_or_else(|| "1.8.2".into());
                                    format!("Bridge ready · fw {fw}")
                                }
                                Some(h) => health_text(&h).to_string(),
                            }
                        };
                        let target_val = if debug_view().as_deref() == Some("panel-target") {
                            "workstation (Linux)".to_string()
                        } else if let Some((hello, _)) = self.agent() {
                            format!("{} ({})", hello.name, hello.os)
                        } else if let Some(state) = self.clip.as_ref().map(ClipBridge::state) {
                            state.label().to_string()
                        } else {
                            "Not paired".to_string()
                        };

                        egui::Grid::new("clip_rows")
                            .num_columns(2)
                            .spacing(egui::vec2(12.0, 4.0))
                            .min_col_width(64.0)
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("Comet")
                                        .font(egui::FontId::new(
                                            13.0,
                                            egui::FontFamily::Proportional,
                                        ))
                                        .color(theme::MUTED),
                                );
                                ui.label(
                                    egui::RichText::new(comet_val)
                                        .font(egui::FontId::new(
                                            13.0,
                                            egui::FontFamily::Proportional,
                                        ))
                                        .color(theme::TEXT),
                                );
                                ui.end_row();

                                ui.label(
                                    egui::RichText::new("Target")
                                        .font(egui::FontId::new(
                                            13.0,
                                            egui::FontFamily::Proportional,
                                        ))
                                        .color(theme::MUTED),
                                );
                                ui.label(
                                    egui::RichText::new(target_val)
                                        .font(egui::FontId::new(
                                            13.0,
                                            egui::FontFamily::Proportional,
                                        ))
                                        .color(theme::TEXT),
                                );
                                ui.end_row();
                            });

                        if let Some(label) = self.bridge_action()
                            && theme::secondary_button(ui, label, 32.0, false).clicked()
                        {
                            self.confirm =
                                Some(Confirm::BridgeSetup(label.trim_end_matches('…').into()));
                        }

                        // Segmented direction control
                        let mut dir_idx = match self.device.clip_direction {
                            ClipDirection::Both => 0,
                            ClipDirection::ToTarget => 1,
                            ClipDirection::FromTarget => 2,
                        };
                        if theme::segmented(
                            ui,
                            &mut dir_idx,
                            &["Both ways", "To target", "From target"],
                        )
                        .changed()
                        {
                            self.device.clip_direction = match dir_idx {
                                0 => ClipDirection::Both,
                                1 => ClipDirection::ToTarget,
                                _ => ClipDirection::FromTarget,
                            };
                            action = ConsoleAction::TargetChanged;
                        }

                        // Pause sync switch
                        if theme::labelled_switch(ui, "Pause sync", &mut self.clip_paused).changed()
                            && let Some(c) = &self.clip
                        {
                            c.set_settings(self.clip_settings());
                        }

                        // Linux paste preference if target is Linux
                        if self.device.target == TargetOs::Linux
                            && debug_view().as_deref() != Some("panel-target")
                        {
                            ui.label(
                                egui::RichText::new("Insert clipboard pastes with:")
                                    .font(egui::FontId::new(12.0, theme::bold_family()))
                                    .color(theme::MUTED),
                            );
                            let before = self.device.insert_paste;
                            for (p, l) in [
                                (PastePref::Auto, "Auto (Ctrl+Shift+V in terminals)"),
                                (PastePref::CtrlV, "Ctrl+V"),
                                (PastePref::CtrlShiftV, "Ctrl+Shift+V"),
                            ] {
                                if ui
                                    .radio_value(&mut self.device.insert_paste, p, l)
                                    .clicked()
                                    && self.device.insert_paste != before
                                {
                                    action = ConsoleAction::TargetChanged;
                                }
                            }
                        }

                        if let Some(c) = &self.clip
                            && c.settings() != self.clip_settings()
                        {
                            c.set_settings(self.clip_settings());
                        }

                        // Set up the target again… button
                        let setup_label = if self.clip.is_none() {
                            "Set up the target…"
                        } else {
                            "Set up the target again…"
                        };
                        if theme::secondary_button(ui, setup_label, 36.0, true)
                            .on_hover_text(
                                "Reinstalls the agent on the target with a new pairing code",
                            )
                            .clicked()
                        {
                            self.start_target_setup();
                        }

                        if self.clip.is_some()
                            && debug_view().as_deref() != Some("panel-target")
                            && ui.add(theme::ghost("Forget pairing")).clicked()
                        {
                            self.forget_clipboard_pairing();
                        }
                    });
            }
        });

        action
    }
}

pub(super) fn health_text(h: &Health) -> String {
    match h {
        Health::NotInstalled => "the Comet has no clipboard bridge".into(),
        Health::Outdated { installed } => format!("the Comet's bridge is outdated ({installed})"),
        Health::Broken(why) => format!("the Comet's bridge is broken: {why}"),
        Health::Ok => "the Comet's bridge is ready".into(),
    }
}
