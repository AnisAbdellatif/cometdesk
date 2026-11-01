//! Target agent pairing and installation modal dialog.
//! Entry points: `Console::start_target_setup`, `Console::target_setup_dialog`.

use super::Console;
use super::target::{agent_name, install_command};
use crate::clipsync::{BridgeState, ClipBridge};
use crate::store::{self, TargetOs};
use crate::theme;

impl Console {
    /// New pairing code (replacing any old pairing) and the dialog that installs the agent
    /// on the target with it.
    pub(super) fn start_target_setup(&mut self) {
        let secret = cometdesk_clip::Secret::generate();
        // The keychain write can block on a macOS ACL prompt: store the new code off the UI
        // thread. The bridge uses the in-memory copy, so it starts regardless.
        let (device, stored) = (self.device.clone(), secret.clone());
        self.rt.spawn_blocking(move || {
            if let Err(e) = store::set_clip_secret(&device, &stored) {
                tracing::warn!(%e, "could not store the pairing code");
            }
        });
        self.target_setup = Some(secret.to_code());
        self.start_clip(secret);
    }

    /// Installs, pairs and starts the agent on the target: one command, typed for the user.
    pub(super) fn target_setup_dialog(&mut self, ctx: &egui::Context) {
        let Some(code) = self.target_setup.clone() else {
            return;
        };
        let os = self.device.target;
        let mut chosen = os;
        let command = install_command(os, &code);
        let agent = agent_name(os);
        let missing_agent = agent.is_some_and(|a| !self.setup.has_agent(a));
        let state = self.clip.as_ref().map(ClipBridge::state);
        let connected = matches!(state, Some(BridgeState::Connected(..)));
        let mut close = false;
        egui::Modal::new(egui::Id::unique("target-setup"))
            .frame(theme::modal_frame())
            .show(ctx, |ui| {
                ui.set_max_width(560.0);
                ui.label(
                    egui::RichText::new("Set up the target")
                        .font(egui::FontId::new(18.0, theme::bold_family()))
                        .color(theme::TEXT),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("The target runs:")
                            .font(egui::FontId::new(13.5, egui::FontFamily::Proportional))
                            .color(theme::MUTED),
                    );
                    for os in [TargetOs::Linux, TargetOs::Windows, TargetOs::MacOs] {
                        ui.selectable_value(&mut chosen, os, os.label());
                    }
                });
                if connected {
                    ui.colored_label(
                        theme::OK,
                        "The agent is connected: the clipboard is shared now.",
                    );
                }
                let Some(command) = &command else {
                    ui.label(
                        egui::RichText::new(format!(
                            "The clipboard agent does not run on {} targets yet. \
                             Pick the target's OS in the toolbar if it is wrong.",
                            os.label()
                        ))
                        .color(theme::MUTED),
                    );
                    ui.label(
                        egui::RichText::new("Manual pairing on a supported system:")
                            .color(theme::MUTED),
                    );
                    ui.monospace(format!("cometdesk-agent pair {code}"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                    return;
                };
                if missing_agent {
                    ui.colored_label(
                        theme::WARN,
                        "The Comet has no agent program for this system yet: build it with \
                         scripts/build-agents.sh, then update the Comet's bridge.",
                    );
                }
                ui.label(
                    egui::RichText::new(format!(
                        "1. Open a {} on the target and click into it.",
                        if os == TargetOs::Windows {
                            "Run dialog or PowerShell"
                        } else {
                            "terminal"
                        }
                    ))
                    .color(theme::MUTED),
                );
                let (open_label, open_keys): (&str, &[&'static str]) = match os {
                    TargetOs::Windows => ("Open the Run dialog (Win+R)", &["MetaLeft", "KeyR"]),
                    _ => (
                        "Open a terminal (Ctrl+Alt+T)",
                        &["ControlLeft", "AltLeft", "KeyT"],
                    ),
                };
                if ui.button(open_label).clicked() {
                    self.send_combo(open_keys);
                }
                ui.label(
                    egui::RichText::new(
                        "2. Run this command there. It downloads the agent from the Comet over the \
                              USB cable, pairs it and starts it (also at every login). No admin rights \
                              or internet needed.",
                    )
                    .color(theme::MUTED),
                );
                ui.add(
                    egui::TextEdit::multiline(&mut command.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    if ui
                        .button("Run it on the target")
                        .on_hover_text("Types the command and presses Enter in the focused window")
                        .clicked()
                    {
                        self.start_typing(format!("{command}\n"));
                    }
                    if ui.button("Copy").clicked() {
                        ui.ctx().copy_text(command.clone());
                    }
                });
                if let Some(s) = &state
                    && !connected
                {
                    ui.label(
                        egui::RichText::new(format!("Status: {}", s.label())).color(theme::MUTED),
                    );
                }
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(theme::primary(if connected { "Done" } else { "Close" }))
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        if chosen != os {
            self.set_target(chosen);
        }
        if close {
            self.target_setup = None;
        }
    }
}
