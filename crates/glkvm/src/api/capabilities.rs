//! What this device can do, so callers can hide features instead of letting calls fail.
//!
//! ```no_run
//! # async fn f(session: glkvm::Session) -> glkvm::Result<()> {
//! use glkvm::api::capabilities::Feature;
//! let caps = session.capabilities().await?;
//! if caps.supports(Feature::Atx) {
//!     // show power buttons
//! }
//! # Ok(()) }
//! ```

use serde::Deserialize;

use crate::api::info::Firmware;
use crate::api::streamer::StreamerFeatures;
use crate::{Result, Session};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    /// ATX power/reset control (needs the ATX board).
    Atx,
    /// Virtual media (mass storage) emulation.
    Msd,
    WakeOnLan,
    /// Keyboard and mouse emulation.
    Hid,
    /// Relative mouse output (`usb_rel`).
    RelativeMouse,
    H264,
    H265,
    /// JPEG quality / H.264 bitrate control.
    VideoQuality,
    ZeroDelay,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Capabilities {
    pub firmware: Firmware,
    pub atx: bool,
    pub msd: bool,
    pub hid: bool,
    pub mouse_outputs: Vec<String>,
    pub streamer: StreamerFeatures,
}

impl Capabilities {
    pub fn supports(&self, feature: Feature) -> bool {
        match feature {
            Feature::Atx => self.atx,
            Feature::Msd => self.msd,
            // Part of every GL.iNet GLKVM firmware; there is no flag for it.
            Feature::WakeOnLan => true,
            Feature::Hid => self.hid,
            Feature::RelativeMouse => self.mouse_outputs.iter().any(|o| o == "usb_rel"),
            Feature::H264 => self.streamer.h264,
            Feature::H265 => self.streamer.h265,
            Feature::VideoQuality => self.streamer.quality,
            Feature::ZeroDelay => self.streamer.zero_delay,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Enabled {
    enabled: bool,
}

impl Session {
    /// Queries the device (5 small requests, in parallel).
    pub async fn capabilities(&self) -> Result<Capabilities> {
        let (firmware, atx, msd, hid, streamer) = tokio::try_join!(
            self.firmware(),
            self.get::<Enabled>("atx", &[]),
            self.get::<Enabled>("msd", &[]),
            self.hid_state(),
            self.streamer_state(),
        )?;
        Ok(Capabilities {
            firmware,
            atx: atx.enabled,
            msd: msd.enabled,
            hid: hid.enabled,
            mouse_outputs: hid.mouse.outputs.available,
            streamer: streamer.features,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::fixtures;
    use crate::api::hid::HidState;
    use crate::api::streamer::StreamerState;

    #[test]
    fn device_capabilities_from_fixtures() {
        let hid: HidState = fixtures::load("hid");
        let caps = Capabilities {
            firmware: fixtures::load("upgrade_version"),
            atx: fixtures::load::<Enabled>("atx").enabled,
            msd: fixtures::load::<Enabled>("msd").enabled,
            hid: hid.enabled,
            mouse_outputs: hid.mouse.outputs.available,
            streamer: fixtures::load::<StreamerState>("streamer_idle").features,
        };
        // This RM1 has no ATX board wired, but MSD, HID and both codecs.
        assert!(!caps.supports(Feature::Atx));
        for f in [
            Feature::Msd,
            Feature::Hid,
            Feature::RelativeMouse,
            Feature::H264,
            Feature::H265,
        ] {
            assert!(caps.supports(f), "{f:?}");
        }
    }

    #[test]
    fn defaults_support_nothing_device_specific() {
        let caps = Capabilities::default();
        for f in [
            Feature::Atx,
            Feature::Msd,
            Feature::Hid,
            Feature::RelativeMouse,
            Feature::H264,
        ] {
            assert!(!caps.supports(f), "{f:?}");
        }
    }
}
