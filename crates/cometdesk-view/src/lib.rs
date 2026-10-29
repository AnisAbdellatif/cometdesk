//! Shared view layer for cometdesk clients: host input → device HID mapping, the winit key table,
//! and the NV12 video renderer (zero-copy on macOS).

pub mod input;
pub mod keymap;
#[cfg(target_os = "macos")]
pub mod pipeline;
pub mod video;
#[cfg(target_os = "macos")]
pub mod zerocopy;

pub use input::{InputMapper, Rect, ScrollOptions, letterbox, letterbox_in};
pub use keymap::KeyOptions;
pub use video::{RenderError, RenderPath, VideoRenderer};
