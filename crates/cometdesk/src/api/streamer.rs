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

impl StreamerState {
    /// Refresh rate of the HDMI input, or `None` if the streamer is idle or doesn't say.
    pub fn source_fps(&self) -> Option<u32> {
        self.streamer.as_ref().and_then(|rt| rt.source_fps())
    }
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Source {
    pub online: bool,
    pub captured_fps: i64,
    pub desired_fps: i64,
    pub resolution: Resolution,
    /// Input mode as `WIDTHxHEIGHT@HZ`, e.g. `2560x1440@60`.
    pub real_resolution: String,
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

    /// Refresh rate of the HDMI input (from `real_resolution`). Not `captured_fps`: that is
    /// a measured rate that dips below the input rate and follows `desired_fps`.
    pub fn source_fps(&self) -> Option<u32> {
        let (_, hz) = self.source.real_resolution.rsplit_once('@')?;
        hz.trim().parse().ok().filter(|&hz| hz > 0)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SnapshotOptions {
    /// Return the "no signal" image instead of failing when the target is off.
    pub allow_offline: bool,
    /// Scale down to fit `(max_width, max_height)` (thumbnails).
    pub preview: Option<(u32, u32)>,
}

/// Encoder settings to change. `None` leaves a value alone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StreamerUpdate {
    pub desired_fps: Option<i64>,
    /// kbit/s.
    pub h264_bitrate: Option<i64>,
    pub h264_gop: Option<i64>,
    pub quality: Option<i64>,
}

/// Named bitrate levels. They are ours, not GL's: the bitrate is placed inside the device's
/// reported `h264_bitrate` limits, so they fit any firmware. The frame rate is set on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityPreset {
    Low,
    Medium,
    High,
    Max,
}

impl QualityPreset {
    pub const ALL: [QualityPreset; 4] = [
        QualityPreset::Low,
        QualityPreset::Medium,
        QualityPreset::High,
        QualityPreset::Max,
    ];

    /// H.264 bitrate in kbit/s for this level.
    pub fn bitrate(self, limits: &StreamerLimits) -> i64 {
        let pct = match self {
            QualityPreset::Low => 15,
            QualityPreset::Medium => 40,
            QualityPreset::High => 70,
            QualityPreset::Max => 100,
        };
        // The limits come from the device, so guard against an overflowing range.
        let b = limits.h264_bitrate;
        let span = b.max.saturating_sub(b.min);
        b.min.saturating_add(span.saturating_mul(pct) / 100)
    }
}

impl StreamerUpdate {
    /// Clamps values to the device's limits.
    pub fn clamped(mut self, l: &StreamerLimits) -> Self {
        let clamp = |v: Option<i64>, r: Range| v.map(|v| v.clamp(r.min, r.max.max(r.min)));
        self.desired_fps = clamp(self.desired_fps, l.desired_fps);
        self.h264_bitrate = clamp(self.h264_bitrate, l.h264_bitrate);
        self.h264_gop = clamp(self.h264_gop, l.h264_gop);
        self.quality = self.quality.map(|q| q.clamp(1, 100));
        self
    }

    fn query(&self) -> Vec<(&'static str, String)> {
        [
            ("desired_fps", self.desired_fps),
            ("h264_bitrate", self.h264_bitrate),
            ("h264_gop", self.h264_gop),
            ("quality", self.quality),
        ]
        .into_iter()
        .filter_map(|(k, v)| Some((k, v?.to_string())))
        .collect()
    }
}

impl Session {
    /// Changes encoder settings. **Device-wide and persisted**: every viewer is affected.
    /// Values are clamped to the device's limits; returns the state re-read afterwards.
    pub async fn set_streamer_params(&self, update: StreamerUpdate) -> Result<StreamerState> {
        let limits = self.streamer_state().await?.limits;
        let query = update.clamped(&limits).query();
        let query: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.post::<serde_json::Value>("streamer/set_params", &query)
            .await?;
        self.streamer_state().await
    }

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
        assert_eq!(s.source_fps(), None);
        assert!(s.features.h264 && s.features.h265 && !s.features.resolution);
        assert_eq!(s.limits.h264_bitrate, Range { min: 0, max: 20000 });
        assert_eq!(s.params.venc_mode.as_deref(), Some("smart"));
        assert_eq!(s.params.video_format, Some(0));
    }

    #[test]
    fn parses_running_streamer() {
        let s: StreamerState = fixtures::load("streamer_running");
        assert_eq!(s.source_fps(), Some(60));
        let rt = s.streamer.expect("running");
        assert_eq!(rt.source_fps(), Some(60));
        assert_eq!(StreamerRuntime::default().source_fps(), None);
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

#[cfg(test)]
mod quality_tests {
    use super::*;

    fn limits() -> StreamerLimits {
        StreamerLimits {
            desired_fps: Range { min: 5, max: 60 },
            h264_bitrate: Range {
                min: 1000,
                max: 21000,
            },
            h264_gop: Range { min: 1, max: 120 },
        }
    }

    #[test]
    fn presets_stay_inside_limits() {
        for p in QualityPreset::ALL {
            let b = p.bitrate(&limits());
            assert!((1000..=21000).contains(&b), "{p:?}: {b}");
        }
        assert_eq!(QualityPreset::Low.bitrate(&limits()), 4000);
        assert_eq!(QualityPreset::Max.bitrate(&limits()), 21000);
    }

    #[test]
    fn clamps_and_builds_query() {
        let u = StreamerUpdate {
            desired_fps: Some(500),
            h264_bitrate: Some(1),
            quality: Some(0),
            ..Default::default()
        }
        .clamped(&limits());
        assert_eq!(
            u.query(),
            vec![
                ("desired_fps", "60".to_string()),
                ("h264_bitrate", "1000".to_string()),
                ("quality", "1".to_string())
            ]
        );
    }
}
