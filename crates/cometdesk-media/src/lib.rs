//! Video sources and hardware decoding for GLKVM devices.

pub mod annexb;
pub mod audio;
pub mod direct;
mod flexfec;
pub mod janus;
pub mod pion;
mod rtc;
pub mod source;
#[cfg(target_os = "macos")]
pub mod vt;

use std::time::Instant;

pub use source::{
    FrameTracker, MediaControl, MediaSession, MediaStats, SourceOptions, SourceState, Transport,
    start,
};

use bytes::Bytes;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Codec {
    H264,
    H265,
}

impl Codec {
    /// Name for people (`H.264`, `H.265`).
    pub fn label(self) -> &'static str {
        match self {
            Codec::H264 => "H.264",
            Codec::H265 => "H.265",
        }
    }

    /// Format name used by the device (`h264`, `h265`).
    pub fn wire_name(self) -> &'static str {
        match self {
            Codec::H264 => "h264",
            Codec::H265 => "h265",
        }
    }
}

impl From<cometdesk::api::system::VideoFormat> for Codec {
    fn from(vf: cometdesk::api::system::VideoFormat) -> Self {
        match vf {
            cometdesk::api::system::VideoFormat::H264 => Codec::H264,
            cometdesk::api::system::VideoFormat::H265 => Codec::H265,
        }
    }
}

impl From<Codec> for cometdesk::api::system::VideoFormat {
    fn from(c: Codec) -> Self {
        match c {
            Codec::H264 => cometdesk::api::system::VideoFormat::H264,
            Codec::H265 => cometdesk::api::system::VideoFormat::H265,
        }
    }
}

/// One Annex B access unit as received from the device.
#[derive(Debug, Clone)]
pub struct EncodedFrame {
    pub codec: Codec,
    pub data: Bytes,
    pub keyframe: bool,
    pub received_at: Instant,
}
