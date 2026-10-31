//! Hardware capability flags (`GET /api/system/capability`).
//!
//! `GET /api/system/clients` (viewer list) exists but answers 403 to normal sessions on
//! V1.10.1; it is reserved for the device's own GUI, so it is not exposed here.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::client::{Body, parse_envelope};
use crate::{Error, Result, Session};

/// The device's saved video mode (`system/get_config` → `video_mode`). **Device-wide**:
/// it decides which video servers run, for every viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoMode {
    /// WebRTC with FlexFEC (Pion, the default).
    FlexFec,
    /// Classic WebRTC (Janus).
    Webrtc,
    /// GL's native "adaptive" WebRTC. Janus is unavailable while active.
    GlWebrtc,
    DirectH264,
}

impl VideoMode {
    pub const ALL: [VideoMode; 4] = [
        VideoMode::FlexFec,
        VideoMode::Webrtc,
        VideoMode::GlWebrtc,
        VideoMode::DirectH264,
    ];

    pub fn wire_name(self) -> &'static str {
        match self {
            VideoMode::FlexFec => "flex_fec",
            VideoMode::Webrtc => "webrtc",
            VideoMode::GlWebrtc => "gl_webrtc",
            VideoMode::DirectH264 => "direct_h264",
        }
    }

    pub fn parse(s: &str) -> Option<VideoMode> {
        Self::ALL.into_iter().find(|m| m.wire_name() == s)
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct CapabilityResult {
    capability: BTreeMap<String, String>,
}

impl Session {
    /// Raw hardware flags (`cpu_model`, `mipi`, `usb_version`, ...). Values are strings.
    pub async fn hardware_capability(&self) -> Result<BTreeMap<String, String>> {
        Ok(self
            .get::<CapabilityResult>("system/capability", &[])
            .await?
            .capability)
    }

    /// The saved video mode, `None` if the device reports an unknown value.
    pub async fn video_mode(&self) -> Result<Option<VideoMode>> {
        Ok(self
            .system_config()
            .await?
            .video_mode
            .as_deref()
            .and_then(VideoMode::parse))
    }

    /// The saved video format (H.264 or H.265), `None` if unset or unknown.
    pub async fn video_format(&self) -> Result<Option<VideoFormat>> {
        let cfg = self.system_config().await?;
        if let Some(format) = cfg.video_format.and_then(VideoFormat::from_wire) {
            return Ok(Some(format));
        }
        if let Ok(st) = self.streamer_state().await
            && let Some(format) = st.params.video_format.and_then(VideoFormat::from_wire)
        {
            return Ok(Some(format));
        }
        Ok(None)
    }

    /// Switches the hardware encoder video format. **Device-wide and persisted**
    /// in both the system config and the streamer params.
    pub async fn set_video_format(&self, format: VideoFormat) -> Result<()> {
        self.set_config_field("video_format", format.wire_value().into())
            .await?;
        self.set_stream_param("video_format", &format.wire_value().to_string())
            .await
    }

    /// Switches the saved video mode. **Device-wide and persisted.** See
    /// [`Session::set_config_field`] for the write semantics.
    pub async fn set_video_mode(&self, mode: VideoMode) -> Result<()> {
        self.set_config_field("video_mode", mode.wire_name().into())
            .await
    }

    /// Reads `system/get_config`, replaces one field and writes the object back (the API
    /// replaces the whole object), then re-reads to confirm. **Device-wide and persisted**:
    /// a change made by another viewer in between is overwritten.
    pub async fn set_config_field(&self, key: &str, value: Value) -> Result<()> {
        #[derive(Deserialize)]
        struct Cfg {
            config: Map<String, Value>,
        }
        let mut cfg: Cfg = self.get("system/get_config", &[]).await?;
        cfg.config.insert(key.into(), value.clone());
        let body = Value::Object(cfg.config).to_string();
        let (status, bytes) = self
            .request(
                reqwest::Method::POST,
                "system/set_config",
                &[],
                Body::Json(&body),
            )
            .await?;
        parse_envelope::<Value>(status, &bytes)?;
        let after: Cfg = self.get("system/get_config", &[]).await?;
        match after.config.get(key) {
            Some(v) if *v == value => Ok(()),
            other => Err(Error::Protocol(format!(
                "device kept {key} = {other:?} after set_config"
            ))),
        }
    }

    /// Rotates the picture. **Device-wide and persisted.** WebRTC sources rotate on the
    /// device (the angle is sent when the stream starts); Direct frames are rotated by the
    /// viewer.
    pub async fn set_orientation(&self, o: Orientation) -> Result<()> {
        self.set_config_field("orientation", o.degrees().into())
            .await
    }

    /// Smart/normal encoder mode. **Device-wide and persisted** in both the config and the
    /// encoder parameters, as the web app does.
    pub async fn set_venc_mode(&self, mode: VencMode) -> Result<()> {
        self.set_config_field("venc_mode", mode.wire_name().into())
            .await?;
        self.set_stream_param("venc_mode", mode.wire_name()).await
    }

    /// Latency/quality trade-off. **Device-wide and persisted**: the config keeps the choice
    /// and the encoder's `zero_delay` flag follows it.
    pub async fn set_latency_mode(&self, mode: LatencyMode) -> Result<()> {
        self.set_config_field("video_processing", mode.wire_name().into())
            .await?;
        let zero_delay = if mode == LatencyMode::LowestLatency {
            "true"
        } else {
            "false"
        };
        self.set_stream_param("zero_delay", zero_delay).await
    }

    async fn set_stream_param(&self, key: &str, value: &str) -> Result<()> {
        self.post::<Value>("streamer/set_params", &[(key, value)])
            .await?;
        Ok(())
    }
}

/// Picture rotation (`system/get_config` → `orientation`, degrees).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Orientation {
    #[default]
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}

impl Orientation {
    pub const ALL: [Orientation; 4] = [
        Orientation::Deg0,
        Orientation::Deg90,
        Orientation::Deg180,
        Orientation::Deg270,
    ];

    pub fn degrees(self) -> i32 {
        match self {
            Orientation::Deg0 => 0,
            Orientation::Deg90 => 90,
            Orientation::Deg180 => 180,
            Orientation::Deg270 => 270,
        }
    }

    pub fn from_degrees(d: i32) -> Option<Orientation> {
        Self::ALL.into_iter().find(|o| o.degrees() == d)
    }
}

/// Encoder mode (`venc_mode`). Smart reduces bandwidth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VencMode {
    Smart,
    Normal,
}

impl VencMode {
    pub const ALL: [VencMode; 2] = [VencMode::Smart, VencMode::Normal];

    pub fn wire_name(self) -> &'static str {
        match self {
            VencMode::Smart => "smart",
            VencMode::Normal => "normal",
        }
    }

    pub fn parse(s: &str) -> Option<VencMode> {
        Self::ALL.into_iter().find(|m| m.wire_name() == s)
    }
}

/// `video_processing`: web UI "Latency Mode".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatencyMode {
    /// Lowest latency (the encoder's `zero_delay`).
    LowestLatency,
    /// Smooth display: steadier picture, more delay.
    SmoothDisplay,
}

impl LatencyMode {
    pub const ALL: [LatencyMode; 2] = [LatencyMode::LowestLatency, LatencyMode::SmoothDisplay];

    pub fn wire_name(self) -> &'static str {
        match self {
            LatencyMode::LowestLatency => "low_latency_first",
            LatencyMode::SmoothDisplay => "quality_first",
        }
    }

    pub fn parse(s: &str) -> Option<LatencyMode> {
        Self::ALL.into_iter().find(|m| m.wire_name() == s)
    }
}

/// Hardware encoder video format (`video_format`: 0 = H.264, 1 = H.265).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoFormat {
    H264,
    H265,
}

impl VideoFormat {
    pub const ALL: [VideoFormat; 2] = [VideoFormat::H264, VideoFormat::H265];

    pub fn wire_value(self) -> i64 {
        match self {
            VideoFormat::H264 => 0,
            VideoFormat::H265 => 1,
        }
    }

    pub fn from_wire(v: i64) -> Option<VideoFormat> {
        match v {
            0 => Some(VideoFormat::H264),
            1 => Some(VideoFormat::H265),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            VideoFormat::H264 => "h264",
            VideoFormat::H265 => "h265",
        }
    }

    /// Name for people (`H.264`, `H.265`).
    pub fn label(self) -> &'static str {
        match self {
            VideoFormat::H264 => "H.264",
            VideoFormat::H265 => "H.265",
        }
    }

    pub fn parse(s: &str) -> Option<VideoFormat> {
        match s.to_ascii_lowercase().as_str() {
            "h264" | "h.264" | "0" => Some(VideoFormat::H264),
            "h265" | "h.265" | "hevc" | "1" => Some(VideoFormat::H265),
            _ => None,
        }
    }
}

impl std::fmt::Display for VideoFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::fixtures;

    #[test]
    fn parses_capability() {
        let c: CapabilityResult = fixtures::load("system_capability");
        assert_eq!(c.capability["cpu_model"], "rv1126");
        assert_eq!(c.capability["usb_version"], "2");
    }

    #[test]
    fn video_format_wire_and_parse() {
        assert_eq!(VideoFormat::H264.wire_value(), 0);
        assert_eq!(VideoFormat::H265.wire_value(), 1);

        assert_eq!(VideoFormat::from_wire(0), Some(VideoFormat::H264));
        assert_eq!(VideoFormat::from_wire(1), Some(VideoFormat::H265));
        assert_eq!(VideoFormat::from_wire(2), None);
        assert_eq!(VideoFormat::from_wire(-1), None);

        assert_eq!(VideoFormat::H264.name(), "h264");
        assert_eq!(VideoFormat::H265.name(), "h265");

        assert_eq!(VideoFormat::parse("h264"), Some(VideoFormat::H264));
        assert_eq!(VideoFormat::parse("H264"), Some(VideoFormat::H264));
        assert_eq!(VideoFormat::parse("h.264"), Some(VideoFormat::H264));
        assert_eq!(VideoFormat::parse("0"), Some(VideoFormat::H264));

        assert_eq!(VideoFormat::parse("h265"), Some(VideoFormat::H265));
        assert_eq!(VideoFormat::parse("H265"), Some(VideoFormat::H265));
        assert_eq!(VideoFormat::parse("h.265"), Some(VideoFormat::H265));
        assert_eq!(VideoFormat::parse("hevc"), Some(VideoFormat::H265));
        assert_eq!(VideoFormat::parse("1"), Some(VideoFormat::H265));

        assert_eq!(VideoFormat::parse("vp9"), None);
        assert_eq!(VideoFormat::parse(""), None);
    }
}
