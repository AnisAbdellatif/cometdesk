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

    /// Switches the saved video mode. **Device-wide and persisted.** `set_config` replaces
    /// the whole object, so this reads it, changes only `video_mode` and writes it back, then
    /// re-reads to confirm. A change made by another viewer in between is overwritten.
    pub async fn set_video_mode(&self, mode: VideoMode) -> Result<()> {
        #[derive(Deserialize)]
        struct Cfg {
            config: Map<String, Value>,
        }
        let mut cfg: Cfg = self.get("system/get_config", &[]).await?;
        cfg.config
            .insert("video_mode".into(), mode.wire_name().into());
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
        match self.video_mode().await? {
            Some(m) if m == mode => Ok(()),
            other => Err(Error::Protocol(format!(
                "device kept video_mode {other:?} after set_config"
            ))),
        }
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
}
