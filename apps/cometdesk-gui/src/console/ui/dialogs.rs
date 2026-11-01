//! Confirmation modal dialogs for typing, ATX power, OS session actions, bridge setup and device video changes.
//! Entry point: `Console::dialogs`.

use cometdesk_clip::PowerAction as SessionAction;

use crate::console::Console;
use crate::console::state::{Confirm, VideoChange};
use crate::theme;

impl Console {
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        self.target_setup_dialog(ctx);
        let Some(confirm) = &self.confirm else { return };
        let (title, body) = match confirm {
            Confirm::Type { reason, .. } => (
                "Type the clipboard on the target?".to_string(),
                format!("The text has {reason}."),
            ),
            Confirm::Power(a) => (
                format!("ATX power: {}?", a.as_str()),
                "This acts on the target's power button.".into(),
            ),
            Confirm::Click(b) => (
                format!("Press the ATX {} button?", b.as_str()),
                "This acts on the target directly.".into(),
            ),
            Confirm::Session(a) => (
                format!("{} the target?", a.label()),
                match a {
                    SessionAction::Logoff => "Programs on the target are closed; unsaved work may be lost.",
                    SessionAction::Restart => "The target restarts; unsaved work may be lost, and the agent reconnects after boot.",
                    _ => "The target powers off; unsaved work may be lost. Turn it back on with the ATX power menu.",
                }
                .into(),
            ),
            Confirm::BridgeSetup(what) => (
                format!("{what}?"),
                "Installs the clipboard bridge on the Comet through its web terminal and \
                 enables its USB network link to the target. If that link has to be (re)created, \
                 the target's keyboard and mouse disconnect for a moment. Takes about 20 seconds."
                    .into(),
            ),
            Confirm::Video(c) => match c {
                VideoChange::Mode(m) => (
                    format!("Switch the video mode to {}?", m.wire_name()),
                    "This is saved on the device and affects every viewer, including the web \
                     interface. Other viewers may need to reconnect."
                        .into(),
                ),
                VideoChange::Encoder(u) => (
                    format!(
                        "Set the encoder to {}?",
                        [
                            u.desired_fps.map(|v| format!("{v} fps")),
                            u.h264_bitrate.map(|v| format!("{v} kbit/s")),
                            u.h264_gop.map(|v| format!("a keyframe every {v} frames")),
                        ]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(", ")
                    ),
                    "This is saved on the device and affects every viewer's bitrate and frame rate."
                        .into(),
                ),
                VideoChange::Codec(f) => (
                    format!(
                        "Switch the video codec to {}?",
                        f.label()
                    ),
                    "This is saved on the device and affects every viewer. Switching codec restarts \
                     the encoder for a few seconds. WebRTC with error correction can't carry H.265; \
                     viewers on it switch to Direct."
                        .into(),
                ),
                VideoChange::Venc(m) => (
                    format!("Switch the encoder to {} mode?", m.wire_name()),
                    "This is saved on the device and affects every viewer.".into(),
                ),
                VideoChange::Latency(m) => (
                    format!("Switch to {}?", m.wire_name().replace('_', " ")),
                    "This is saved on the device and affects every viewer.".into(),
                ),
                VideoChange::Orientation(o) => (
                    format!("Rotate the picture to {}°?", o.degrees()),
                    "This is saved on the device and affects every viewer, including the web \
                     interface."
                        .into(),
                ),
                VideoChange::Edid(p) => (
                    format!("Present the target a {} display?", p.label),
                    "The target sees a monitor change and may rearrange its windows or \
                     resolution. This is saved on the device and affects every viewer."
                        .into(),
                ),
            },
        };
        let mut decision = None;
        egui::Modal::new(egui::Id::unique("confirm"))
            .frame(theme::modal_frame())
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(
                    egui::RichText::new(title)
                        .font(egui::FontId::new(18.0, theme::bold_family()))
                        .color(theme::TEXT),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(body)
                        .font(egui::FontId::new(13.5, egui::FontFamily::Proportional))
                        .color(theme::MUTED),
                );
                ui.add_space(12.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(theme::primary("Continue")).clicked() {
                        decision = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        decision = Some(false);
                    }
                });
            });
        match decision {
            Some(true) => match self.confirm.take() {
                Some(Confirm::Type { text, .. }) => self.deliver_text(text),
                Some(Confirm::BridgeSetup(_)) => self.setup.run(),
                Some(Confirm::Video(c)) => self.apply_video(c),
                Some(Confirm::Session(a)) => self.session_action(a),
                Some(c) => self.atx(c),
                None => {}
            },
            Some(false) => self.confirm = None,
            None => {}
        }
    }
}
