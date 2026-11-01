//! The Video tab in the right-hand drawer: viewer transport/audio and whole-device encoder settings.
//! Entry point: `Console::video_menu`.

use cometdesk::api::edid;
use cometdesk::api::streamer::StreamerUpdate;
use cometdesk::api::system::{LatencyMode, Orientation, VencMode, VideoFormat, VideoMode};
use cometdesk_media::Transport;

use super::widgets::menu_button_item;
use crate::console::Console;
use crate::console::state::{Confirm, VideoChange};
use crate::console::video::video_mode_label;
use crate::theme;

impl Console {
    pub(in crate::console) fn video_menu(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 12.0);

        // --- THIS VIEWER section ---
        theme::caption(ui, "THIS VIEWER");

        theme::field_label(ui, "Video source");
        let mut trans_idx = match self.transport {
            Transport::Direct => 0,
            Transport::Pion => 1,
            Transport::Janus => 2,
        };
        if theme::segmented(ui, &mut trans_idx, &["Direct", "WebRTC", "Janus"]).changed() {
            let t = match trans_idx {
                0 => Transport::Direct,
                1 => Transport::Pion,
                _ => Transport::Janus,
            };
            self.set_transport(t);
        }

        let mut audio = self.audio_on;
        let audio_enabled = matches!(
            self.video_info.mode,
            Some(VideoMode::Webrtc | VideoMode::FlexFec | VideoMode::DirectH264)
        );
        let audio_resp = ui.add_enabled_ui(audio_enabled, |ui| {
            theme::labelled_switch(ui, "Play audio", &mut audio)
        });
        if audio_resp.inner.changed() && audio_enabled {
            self.set_audio(audio);
        }
        if !audio_enabled {
            audio_resp.response.on_disabled_hover_text(
                "Audio needs the device in webrtc, flex_fec or direct_h264 video mode",
            );
        }

        let devices = self.audio_outputs();
        let current_out = if self.device.audio_output.is_empty() {
            "System default"
        } else {
            &self.device.audio_output
        };
        let mut chosen_audio = None;
        theme::show_dropdown_row(
            ui,
            egui::Id::unique("audio_output_dropdown"),
            "Audio output",
            current_out,
            240.0,
            |ui| {
                if menu_button_item(ui, "System default", None).clicked() {
                    chosen_audio = Some(String::new());
                    ui.close();
                }
                for name in &devices {
                    if menu_button_item(ui, name, None).clicked() {
                        chosen_audio = Some(name.clone());
                        ui.close();
                    }
                }
            },
        );
        if let Some(name) = chosen_audio {
            self.device.audio_output = name;
            self.target_changed = true;
            self.set_audio(true);
        }

        ui.add_space(4.0);

        // --- Whole device card (orange outline) ---
        let card_frame = theme::panel_card()
            .stroke(egui::Stroke::new(1.0, theme::WARN))
            .inner_margin(egui::Margin::same(14));

        card_frame.show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 10.0);

            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 2.0);
                ui.label(
                    egui::RichText::new("Whole device")
                        .font(egui::FontId::new(13.0, theme::bold_family()))
                        .color(theme::WARN),
                );
                ui.label(
                    egui::RichText::new("Applies to every viewer, after you confirm.")
                        .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                        .color(theme::MUTED),
                );
            });

            let info = &self.video_info;
            let mut change = None;

            // Codec dropdown row
            let codec_val = info.codec.map_or("H.264", |c| c.label());
            theme::show_dropdown_row(
                ui,
                egui::Id::unique("codec_dropdown"),
                "Codec",
                codec_val,
                200.0,
                |ui| {
                    for format in VideoFormat::ALL {
                        let on = info.codec == Some(format);
                        let supported = match format {
                            VideoFormat::H264 => true,
                            VideoFormat::H265 => info.h265_supported,
                        };
                        if supported && menu_button_item(ui, format.label(), None).clicked() && !on
                        {
                            change = Some(VideoChange::Codec(format));
                            ui.close();
                        }
                    }
                },
            );

            // Latency dropdown row
            let latency_val = match info.latency {
                Some(LatencyMode::LowestLatency) => "Lowest",
                Some(LatencyMode::SmoothDisplay) => "Smooth",
                None => "Lowest",
            };
            theme::show_dropdown_row(
                ui,
                egui::Id::unique("latency_dropdown"),
                "Latency",
                latency_val,
                200.0,
                |ui| {
                    for (m, label) in [
                        (LatencyMode::LowestLatency, "Lowest"),
                        (LatencyMode::SmoothDisplay, "Smooth"),
                    ] {
                        let on = info.latency == Some(m);
                        if menu_button_item(ui, label, None).clicked() && !on {
                            change = Some(VideoChange::Latency(m));
                            ui.close();
                        }
                    }
                },
            );

            // Resolution dropdown row
            let cur_preset = info
                .edid_presets
                .iter()
                .find(|p| edid::normalize(&p.content) == info.edid);
            let res_val = cur_preset.map(|p| p.label.as_str()).unwrap_or("1920×1080");
            theme::show_dropdown_row(
                ui,
                egui::Id::unique("resolution_dropdown"),
                "Resolution",
                res_val,
                240.0,
                |ui| {
                    for p in &info.edid_presets {
                        let on = cur_preset.is_some_and(|c| c.key == p.key);
                        let label = if p.audio {
                            p.label.clone()
                        } else {
                            format!("{} (no audio)", p.label)
                        };
                        if menu_button_item(ui, &label, None).clicked() && !on {
                            change = Some(VideoChange::Edid(p.clone()));
                            ui.close();
                        }
                    }
                },
            );

            // Streaming protocol dropdown row
            let mode_val = info.mode.map_or("Direct H.264", video_mode_label);
            theme::show_dropdown_row(
                ui,
                egui::Id::unique("mode_dropdown"),
                "Streaming protocol",
                mode_val,
                220.0,
                |ui| {
                    for m in VideoMode::ALL {
                        let on = info.mode == Some(m);
                        if menu_button_item(ui, video_mode_label(m), Some(m.wire_name())).clicked()
                            && !on
                        {
                            change = Some(VideoChange::Mode(m));
                            ui.close();
                        }
                    }
                },
            );

            // Bandwidth saving dropdown row
            let venc_val = match info.venc {
                Some(VencMode::Smart) => "Smart",
                Some(VencMode::Normal) => "Normal",
                None => "Normal",
            };
            theme::show_dropdown_row(
                ui,
                egui::Id::unique("venc_dropdown"),
                "Bandwidth saving",
                venc_val,
                200.0,
                |ui| {
                    for (m, label) in [(VencMode::Smart, "Smart"), (VencMode::Normal, "Normal")] {
                        let on = info.venc == Some(m);
                        if menu_button_item(ui, label, None).clicked() && !on {
                            change = Some(VideoChange::Venc(m));
                            ui.close();
                        }
                    }
                },
            );

            // Orientation dropdown row
            let orient_val = format!("{}°", info.orientation.degrees());
            theme::show_dropdown_row(
                ui,
                egui::Id::unique("orient_dropdown"),
                "Orientation",
                &orient_val,
                160.0,
                |ui| {
                    for o in Orientation::ALL {
                        let on = info.orientation == o;
                        let label = format!("{}°", o.degrees());
                        if menu_button_item(ui, &label, None).clicked() && !on {
                            change = Some(VideoChange::Orientation(o));
                            ui.close();
                        }
                    }
                },
            );

            // Monospace encoder summary line
            let p = &info.params;
            let (cur_fps, cur_kbps, cur_gop) = (p.desired_fps, p.h264_bitrate, p.h264_gop);
            let (f, b) = (info.limits.desired_fps, info.limits.h264_bitrate);
            // Without reported limits a 1..=1 range would clamp the draft to a nonsense value.
            let (fps_min, fps_max) = if f.max > f.min {
                (f.min.max(1), f.max)
            } else {
                (1, 240)
            };
            let (kbps_min, kbps_max) = if b.max > b.min {
                (b.min, b.max)
            } else {
                (100, 100_000)
            };
            let (mut fps, mut kbps, gop) = self.encoder_draft.unwrap_or((
                cur_fps.unwrap_or(60),
                cur_kbps.unwrap_or(8000),
                cur_gop.unwrap_or(60),
            ));

            let summary = format!(
                "Encoder: {} fps · {} kbit/s · keyframe {}",
                cur_fps.unwrap_or(60),
                cur_kbps.unwrap_or(8000),
                cur_gop.unwrap_or(60),
            );
            ui.label(
                egui::RichText::new(summary)
                    .font(egui::FontId::new(11.0, egui::FontFamily::Monospace))
                    .color(theme::MUTED),
            );

            // Equal-width fps and kbit/s fields
            let pair_w = (ui.available_width() - 8.0) * 0.5;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
                theme::field_box(ui, pair_w, "fps", &mut fps, fps_min..=fps_max, 1.0);
                theme::field_box(ui, pair_w, "kbit/s", &mut kbps, kbps_min..=kbps_max, 50.0);
            });

            let update = StreamerUpdate {
                desired_fps: (Some(fps) != cur_fps).then_some(fps),
                h264_bitrate: (Some(kbps) != cur_kbps).then_some(kbps),
                h264_gop: (Some(gop) != cur_gop).then_some(gop),
                ..Default::default()
            };
            let edited = update != StreamerUpdate::default();
            self.encoder_draft =
                (edited || self.encoder_draft.is_some()).then_some((fps, kbps, gop));

            if theme::yellow_button(ui, "Apply…", 40.0).clicked() {
                self.encoder_draft = None;
                let final_update = StreamerUpdate {
                    desired_fps: Some(fps),
                    h264_bitrate: Some(kbps),
                    h264_gop: Some(gop),
                    ..Default::default()
                };
                change = Some(VideoChange::Encoder(final_update));
            }

            if let Some(c) = change {
                self.confirm = Some(Confirm::Video(c));
            }
        });
    }
}
