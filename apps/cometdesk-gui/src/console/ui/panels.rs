//! Target and Settings tab bodies inside the right-hand drawer.
//! Entry points: `Console::target_panel`, `Console::settings_panel`.

use cometdesk_clip::SessionState;

use crate::console::menus::widgets::menu_button_item;
use crate::console::{Console, ConsoleAction, debug_view};
use crate::store::TargetOs;
use crate::theme;

impl Console {
    /// Everything about the machine behind the KVM: clipboard sync, files, session, system
    /// info and virtual media. True when the saved device changed.
    pub(super) fn target_panel(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        let locked = self
            .agent()
            .is_some_and(|(_, i)| matches!(i.session, SessionState::Locked));
        let debug_target = debug_view().as_deref() == Some("panel-target");
        if locked {
            theme::attention_banner(ui, "The target is locked (lock screen).");
        } else if debug_target || (self.clip.is_some() && self.bridge_action().is_some()) {
            theme::attention_banner(ui, "Clipboard needs attention. Open Clipboard below.");
        }
        if self.clip_menu(ui) != ConsoleAction::None {
            changed = true;
        }
        let show_agent_actions =
            self.inserts_via_agent() || debug_view().as_deref() == Some("panel-target");
        if show_agent_actions {
            let btn_w = ((ui.available_width() - 8.0) * 0.5).floor();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.allocate_ui(egui::vec2(btn_w, 40.0), |ui| {
                    if theme::card_button(ui, "Send file…", 40.0)
                        .on_hover_text(
                            "Copy a file to the target's Downloads folder (or drop it on the window)",
                        )
                        .clicked()
                    {
                        self.pick_files_to_send();
                    }
                });
                ui.allocate_ui(egui::vec2(btn_w, 40.0), |ui| {
                    if theme::card_button(ui, "Browse files…", 40.0)
                        .on_hover_text("Browse the target's files and download them to this Mac")
                        .clicked()
                        && let Some(clip) = self.clip.as_ref()
                    {
                        self.browser.open(clip);
                    }
                });
            });
        }
        self.session_menu(ui);
        self.info_menu(ui);
        changed
    }

    /// Target OS, keyboard layout, stats and the device's web interface. True when the saved
    /// device changed.
    pub(super) fn settings_panel(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 12.0);

        // --- TARGET section ---
        theme::caption(ui, "TARGET");

        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 8.0);

            // Target OS dropdown row
            theme::show_dropdown_row(
                ui,
                egui::Id::unique("settings_target_os"),
                "Target OS",
                self.device.target.label(),
                220.0,
                |ui| {
                    for os in TargetOs::ALL {
                        let on = self.device.target == os;
                        if menu_button_item(ui, os.label(), None).clicked() && !on {
                            self.device.target = os;
                            let iso = self.mapper.keys.iso_fix;
                            self.mapper.keys = self.device.target.key_options(iso);
                            changed = true;
                            ui.close();
                        }
                    }
                },
            );

            // Keyboard layout dropdown row
            let current = self.keymap().unwrap_or_else(|| "default".into());
            let on_target: Vec<&'static str> = self
                .agent()
                .map(|(_, i)| {
                    i.layouts
                        .iter()
                        .filter_map(|l| cometdesk::layout::keymap_from_native(l))
                        .collect()
                })
                .unwrap_or_default();

            theme::show_dropdown_row(
                ui,
                egui::Id::unique("settings_keyboard_layout"),
                "Keyboard layout",
                &current,
                260.0,
                |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(280.0)
                        .show(ui, |ui| {
                            for k in &self.keymaps {
                                let note = if on_target.contains(&k.as_str()) {
                                    Some("on target")
                                } else {
                                    None
                                };
                                let on = self.device.keymap.as_deref() == Some(k.as_str());
                                if menu_button_item(ui, k, note).clicked() && !on {
                                    self.device.keymap = Some(k.clone());
                                    changed = true;
                                    ui.close();
                                }
                            }
                        });
                },
            );
        });

        // --- THIS VIEWER section ---
        theme::caption(ui, "THIS VIEWER");

        theme::labelled_switch(ui, "Show stats", &mut self.show_stats)
            .on_hover_text("Right Cmd + S");

        // --- DEVICE section ---
        theme::caption(ui, "DEVICE");

        let device_card = theme::panel_card();
        device_card.show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 10.0);
            ui.label(
                egui::RichText::new("Direct management of the Comet device.")
                    .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                    .color(theme::MUTED),
            );
            if ui
                .add(egui::Button::new("Device web interface…"))
                .on_hover_text("Opens the device's web interface in the browser")
                .clicked()
            {
                let url = format!("https://{}/", self.session.host());
                if let Err(e) = open::that(&url) {
                    self.notify(format!("Cannot open {url}: {e}"));
                }
            }
        });

        changed
    }
}
