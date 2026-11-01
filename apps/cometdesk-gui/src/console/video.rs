//! Device-wide video configuration reads/writes and viewer transport mapping.
//! Entry points: `transport_for_mode`, `Console::refresh_video_info`, `Console::apply_video`.

use cometdesk::Session;
use cometdesk::api::system::{LatencyMode, Orientation, VencMode, VideoFormat, VideoMode};
use cometdesk_media::Transport;

use super::Console;
use super::state::{VideoChange, VideoInfo};
use crate::app::UserEvent;

/// Maps the device's `video_mode` (`COMETDESK_TRANSPORT` overrides it for debugging) to the
/// viewer transport.
pub(super) fn transport_for_mode(mode: Option<VideoMode>) -> Transport {
    if let Some(t) = std::env::var("COMETDESK_TRANSPORT")
        .ok()
        .and_then(|t| t.parse().ok())
    {
        return t;
    }
    match mode {
        Some(VideoMode::FlexFec) => Transport::Pion,
        Some(VideoMode::Webrtc) => Transport::Janus,
        Some(VideoMode::DirectH264 | VideoMode::GlWebrtc) | None => Transport::Direct,
    }
}

/// Reads the device-wide video settings.
async fn read_video_info(session: &Session) -> VideoInfo {
    let (cfg, st, edid, presets) = tokio::join!(
        session.system_config(),
        session.streamer_state(),
        session.edid(),
        session.edid_presets(),
    );
    let cfg = cfg.ok().unwrap_or_default();
    let st = st.unwrap_or_default();
    let codec = cfg.video_format.and_then(VideoFormat::from_wire);
    let h265_supported = st.features.h265;
    VideoInfo {
        codec,
        h265_supported,
        mode: cfg.video_mode.as_deref().and_then(VideoMode::parse),
        venc: cfg.venc_mode.as_deref().and_then(VencMode::parse),
        latency: cfg.video_processing.as_deref().and_then(LatencyMode::parse),
        orientation: cfg
            .orientation
            .and_then(Orientation::from_degrees)
            .unwrap_or_default(),
        edid: edid.unwrap_or_default(),
        edid_presets: presets.unwrap_or_default(),
        params: st.params,
        limits: st.limits,
    }
}

impl Console {
    /// Reads the device-wide video mode and encoder settings in the background.
    pub(super) fn refresh_video_info(&mut self) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.video_info_rx = Some(rx);
        let session = self.session.clone();
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            let _ = tx.send(read_video_info(&session).await);
            let _ = proxy.send_event(UserEvent::Refresh);
        });
    }

    /// Writes a device-wide video setting, then re-reads the state. The read must follow the
    /// write inside one task: as separate tasks they race and the menu shows the old value.
    pub(super) fn apply_video(&mut self, change: VideoChange) {
        let session = self.session.clone();
        let proxy = self.proxy.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.video_info_rx = Some(rx);
        self.rt.spawn(async move {
            let msg = match change {
                VideoChange::Mode(m) => session
                    .set_video_mode(m)
                    .await
                    .map(|()| format!("Video mode: {}", m.wire_name()))
                    .map_err(|e| format!("Cannot set video mode: {e}")),
                VideoChange::Encoder(u) => session
                    .set_streamer_params(u)
                    .await
                    .map(|_| "Encoder quality changed".to_string())
                    .map_err(|e| format!("Cannot set quality: {e}")),
                VideoChange::Codec(f) => session
                    .set_video_format(f)
                    .await
                    .map(|()| format!("Codec: {}", f.label()))
                    .map_err(|e| format!("Cannot set codec: {e}")),
                VideoChange::Venc(m) => session
                    .set_venc_mode(m)
                    .await
                    .map(|()| format!("Encoder mode: {}", m.wire_name()))
                    .map_err(|e| format!("Cannot set encoder mode: {e}")),
                VideoChange::Latency(m) => session
                    .set_latency_mode(m)
                    .await
                    .map(|()| "Latency mode changed".to_string())
                    .map_err(|e| format!("Cannot set latency mode: {e}")),
                VideoChange::Orientation(o) => session
                    .set_orientation(o)
                    .await
                    .map(|()| format!("Orientation: {}°", o.degrees()))
                    .map_err(|e| format!("Cannot set orientation: {e}")),
                VideoChange::Edid(p) => session
                    .set_edid(&p.content)
                    .await
                    .map(|()| format!("Display: {}", p.label))
                    .map_err(|e| format!("Cannot set the display: {e}")),
            };
            let _ = tx.send(read_video_info(&session).await);
            let _ = proxy.send_event(UserEvent::Notice(msg.unwrap_or_else(|e| e)));
        });
    }
}

pub(super) fn video_mode_label(m: VideoMode) -> &'static str {
    match m {
        VideoMode::FlexFec => "WebRTC with error correction",
        VideoMode::Webrtc => "WebRTC",
        VideoMode::GlWebrtc => "GL native (no Janus)",
        VideoMode::DirectH264 => "Direct H.264",
    }
}
