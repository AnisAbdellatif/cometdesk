//! One-shot CLI subcommands grouped by device capability.
//!
//! Re-exports all command entry points so callers use `commands::<fn>`.
//! Read `README.md` in this directory to locate individual command modules.

mod atx;
mod bridge;
mod edid;
mod fixtures;
mod info;
mod msd;
#[cfg(test)]
mod tests;
mod type_text;
mod util;
mod video;
mod wol;

pub use self::atx::{click, power};
pub use self::bridge::{bridge_remove, bridge_setup, bridge_status};
pub use self::edid::edid;
pub use self::fixtures::record_fixtures;
pub use self::info::{info, snapshot};
pub use self::msd::{msd_mount, msd_remove, msd_status, msd_unmount, msd_upload};
pub use self::type_text::type_text;
pub use self::video::{codec, latency_mode, orientation, quality, venc_mode, video_mode};
pub use self::wol::{wake, wol_list};
