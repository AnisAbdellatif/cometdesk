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

pub use source::{SourceOptions, Transport, start};

use bytes::Bytes;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Codec {
    H264,
    H265,
}

impl Codec {
    /// Format name used by the device (`h264`, `h265`).
    pub fn wire_name(self) -> &'static str {
        match self {
            Codec::H264 => "h264",
            Codec::H265 => "h265",
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
