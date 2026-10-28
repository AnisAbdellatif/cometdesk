//! Shared view layer for glkvm clients: host input → device HID mapping, the winit key table,
//! and the NV12 video renderer (zero-copy on macOS).

pub mod input;
pub mod keymap;
pub mod video;
#[cfg(target_os = "macos")]
pub mod zerocopy;

pub use input::{InputMapper, Rect, ScrollOptions, letterbox};
pub use keymap::KeyOptions;
pub use video::{RenderError, RenderPath, VideoRenderer};
