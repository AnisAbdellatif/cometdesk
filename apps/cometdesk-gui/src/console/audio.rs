//! Target audio playback, output device enumeration and master volume controls.
//! Entry points: `Console::start_audio`, `Console::set_audio`, `Console::volume_row`.

use std::time::{Duration, Instant};

use cometdesk_media::audio::Player;

use super::Console;
use super::state::Audio;
use crate::clipsync::Request;

impl Console {
    /// Audio comes from the source matching the device's video mode (Janus in `webrtc`, Pion
    /// in `flex_fec`), independent of how video reaches us, and only while it is switched on.
    pub(super) fn start_audio(&mut self) -> Option<Audio> {
        if !self.audio_on {
            return None;
        }
        let _guard = self.rt.enter();
        // Unknown until the first read of the device state; see where `video_info` arrives.
        let mode = self.video_info.mode?.wire_name();
        let Some(source) = cometdesk_media::audio::start(self.session.clone(), mode) else {
            self.audio_on = false;
            self.notify(
                "Audio needs the device in webrtc, flex_fec or direct_h264 video mode".into(),
            );
            return None;
        };
        let (packets, session) = source.take_packets();
        let output = Some(self.device.audio_output.as_str()).filter(|n| !n.is_empty());
        match Player::start(packets, output) {
            Ok(player) => Some(Audio {
                _player: player,
                _session: session,
            }),
            Err(e) => {
                self.audio_on = false;
                self.notify(format!("Audio unavailable: {e}"));
                None
            }
        }
    }

    pub(super) fn set_audio(&mut self, on: bool) {
        if self.device.audio != on {
            self.device.audio = on;
            self.target_changed = true;
        }
        self.audio_on = on;
        let audio = if on { self.start_audio() } else { None };
        if let Some(s) = &mut self.stream {
            s.audio = audio;
        }
    }

    /// Audio output device names, re-enumerated at most once per [`AUDIO_OUTPUTS_TTL`] so the
    /// submenu doesn't query the OS every frame it is open.
    pub(super) fn audio_outputs(&mut self) -> Vec<String> {
        const AUDIO_OUTPUTS_TTL: Duration = Duration::from_secs(2);
        let fresh = self
            .audio_outputs
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() < AUDIO_OUTPUTS_TTL);
        if !fresh {
            self.audio_outputs = Some((cometdesk_media::audio::output_devices(), Instant::now()));
        }
        self.audio_outputs.as_ref().unwrap().0.clone()
    }

    /// The master volume of the target: a slider that sends on release, and mute.
    pub(super) fn volume_row(&mut self, ui: &mut egui::Ui, volume: cometdesk_clip::Volume) {
        let shown = self
            .volume_edit
            .filter(|(_, at)| at.elapsed() < Duration::from_secs(3))
            .map_or(volume.percent, |(v, _)| v);
        let mut level = f32::from(shown);
        let mut muted = volume.muted;
        ui.horizontal(|ui| {
            ui.label("Volume");
            let slider = ui.add(egui::Slider::new(&mut level, 0.0..=100.0).suffix("%"));
            if slider.changed() {
                self.volume_edit = Some((level as u8, Instant::now()));
            }
            if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
                self.ask_agent(
                    Request::Volume {
                        level: Some(level as u8),
                        muted: None,
                    },
                    "Cannot set the volume",
                    None,
                );
            }
            if ui.checkbox(&mut muted, "Mute").changed() {
                self.ask_agent(
                    Request::Volume {
                        level: None,
                        muted: Some(muted),
                    },
                    "Cannot change the mute",
                    None,
                );
            }
        });
    }
}
