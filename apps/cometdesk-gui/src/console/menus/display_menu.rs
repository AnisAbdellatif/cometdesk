//! Per-display resolution picker, EDID matching and primary display toggle.
//! Entry point: `Console::display_menu`.

use cometdesk::api::edid::{self, EdidPreset};

use crate::clipsync::Request;
use crate::console::Console;
use crate::console::state::VideoChange;
use crate::theme;

impl Console {
    /// Resolution and main-display choices for display `index` of the target.
    pub(in crate::console) fn display_menu(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        d: &cometdesk_clip::Display,
        count: usize,
    ) {
        let can_main = count > 1 && !d.primary;
        if d.modes.is_empty() && !can_main {
            return;
        }
        theme::section(ui, format!("Display {}", index + 1), |ui| {
            if !d.modes.is_empty() {
                ui.label("Resolution");
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for &(w, h) in &d.modes {
                            let on = (w, h) == (d.width, d.height);
                            if ui.selectable_label(on, format!("{w}×{h}")).clicked() && !on {
                                self.ask_agent(
                                    Request::Display {
                                        index: index as u32,
                                        mode: Some((w, h)),
                                        primary: false,
                                    },
                                    "Cannot change the resolution",
                                    Some(format!("Resolution set to {w}×{h}")),
                                );
                                ui.close();
                            }
                        }
                    });
                // The KVM's EDID tells the target which modes the monitor has: when the
                // target runs a mode the current EDID does not name, offer the matching one.
                if let Some(p) = self.edid_for(d.width, d.height)
                    && ui
                        .button(format!("Match the EDID: {}", p.label))
                        .on_hover_text("Tell the target the monitor is this one")
                        .clicked()
                {
                    self.apply_video(VideoChange::Edid(p));
                    ui.close();
                }
            }
            if can_main && ui.button("Make this the main display").clicked() {
                self.ask_agent(
                    Request::Display {
                        index: index as u32,
                        mode: None,
                        primary: true,
                    },
                    "Cannot change the main display",
                    Some(format!("Display {} is now the main display", index + 1)),
                );
                ui.close();
            }
        });
    }

    /// The device's EDID preset named for `width`×`height` (or its `…p`), if it is not the
    /// one in use.
    fn edid_for(&self, width: u32, height: u32) -> Option<EdidPreset> {
        let needles = [
            format!("{width}x{height}"),
            format!("{width}×{height}"),
            format!("{height}p"),
        ];
        let current = &self.video_info.edid;
        self.video_info
            .edid_presets
            .iter()
            .filter(|p| edid::normalize(&p.content) != *current)
            .find(|p| {
                let label = p.label.to_lowercase();
                needles.iter().any(|n| label.contains(n.as_str()))
            })
            .cloned()
    }
}
