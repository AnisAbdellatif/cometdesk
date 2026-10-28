//! Video streamer state, snapshots and parameters (`/api/streamer`).

use bytes::Bytes;
use serde::Deserialize;

use crate::client::{Body, status_error};
use crate::{Error, Result, Session};

/// `GET /api/streamer`. `streamer` is `None` while no viewer keeps the streamer running.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct StreamerState {
    pub features: StreamerFeatures,
    pub limits: StreamerLimits,
    pub params: StreamerParams,
    pub streamer: Option<StreamerRuntime>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct StreamerFeatures {
    pub h264: bool,
    pub h265: bool,
    pub quality: bool,
    pub resolution: bool,
    pub venc_mode: bool,
    pub zero_delay: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Range {
    pub min: i64,
    pub max: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct StreamerLimits {
    pub desired_fps: Range,
    pub h264_bitrate: Range,
    pub h264_gop: Range,
}

/// Saved encoder parameters (**device-wide**).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct StreamerParams {
    pub desired_fps: Option<i64>,
    /// kbit/s.
    pub h264_bitrate: Option<i64>,
    pub h264_gop: Option<i64>,
    pub quality: Option<i64>,
    pub venc_mode: Option<String>,
    /// 0 = H.264, 1 = H.265.
    pub video_format: Option<i64>,
    pub zero_delay: Option<bool>,
}

/// Live state while the streamer runs.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct StreamerRuntime {
    pub encoder: Encoder,
    pub h264: H264State,
    pub hdmi: Hdmi,
    pub source: Source,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Encoder {
    pub quality: i64,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct H264State {
    /// bit/s (note: the params use kbit/s).
    pub bitrate: i64,
    pub fps: i64,
    pub gop: i64,
    pub online: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Hdmi {
    /// The target is sending a picture. False usually means it is off or asleep.
    pub signal: bool,
    pub out_signal: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Source {
    pub online: bool,
    pub captured_fps: i64,
    pub desired_fps: i64,
    pub resolution: Resolution,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl StreamerRuntime {
    /// A picture is actually being captured.
    pub fn has_signal(&self) -> bool {
        self.hdmi.signal && self.source.online
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SnapshotOptions {
    /// Return the "no signal" image instead of failing when the target is off.
    pub allow_offline: bool,
    /// Scale down to fit `(max_width, max_height)` (thumbnails).
    pub preview: Option<(u32, u32)>,
}

impl Session {
    pub async fn streamer_state(&self) -> Result<StreamerState> {
        self.get("streamer", &[]).await
    }

    /// JPEG snapshot of the current picture. Fails with HTTP 503 while the streamer is not
    /// running (no viewer); keep an `/api/ws` session with streaming open.
    pub async fn snapshot(&self, opts: SnapshotOptions) -> Result<Bytes> {
        let (w, h);
        let mut query: Vec<(&str, &str)> =
            vec![("allow_offline", if opts.allow_offline { "1" } else { "0" })];
        if let Some((max_w, max_h)) = opts.preview {
            (w, h) = (max_w.to_string(), max_h.to_string());
            query.extend([
                ("preview", "1"),
                ("preview_max_width", &w),
                ("preview_max_height", &h),
            ]);
        }
        let (status, body) = self
            .request(
                reqwest::Method::GET,
                "streamer/snapshot",
                &query,
                Body::None,
            )
            .await?;
        if status != 200 {
            return Err(status_error(status, &body));
        }
        if !is_jpeg(&body) {
            return Err(Error::Protocol("snapshot is not a JPEG".into()));
        }
        Ok(body)
    }
}

fn is_jpeg(data: &[u8]) -> bool {
    data.starts_with(&[0xff, 0xd8, 0xff])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::fixtures;

    #[test]
    fn parses_idle_streamer() {
        let s: StreamerState = fixtures::load("streamer_idle");
        assert!(s.streamer.is_none());
        assert!(s.features.h264 && s.features.h265 && !s.features.resolution);
        assert_eq!(s.limits.h264_bitrate, Range { min: 0, max: 20000 });
        assert_eq!(s.params.venc_mode.as_deref(), Some("smart"));
        assert_eq!(s.params.video_format, Some(0));
    }

    #[test]
    fn parses_running_streamer() {
        let s: StreamerState = fixtures::load("streamer_running");
        let rt = s.streamer.expect("running");
        assert!(rt.has_signal());
        assert_eq!(rt.encoder.kind, "RV1126-H264");
        assert!(rt.h264.online && rt.h264.fps > 0);
        assert!(rt.source.resolution.width >= 640);
    }

    #[test]
    fn jpeg_detection() {
        assert!(is_jpeg(&[0xff, 0xd8, 0xff, 0xe0]));
        assert!(!is_jpeg(br#"{"ok":false}"#));
    }
}
