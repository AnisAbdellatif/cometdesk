//! Screenshots of the target's own screen, on request from the GUI only: never in the
//! background. Pixel-exact (no H.264, no chroma subsampling, native scale), unlike a frame
//! of the video stream, so text is read better from it.
//!
//! Windows: GDI `BitBlt` of the desktop. X11: `GetImage` on the root window. Wayland lets
//! programs capture only through the desktop portal (a permission prompt), which is not done
//! yet. The lock screen is another desktop: nothing to capture there.

use cometdesk_clip::{Display, SessionState};
use desktop_kit::capture::{Rect, grab};
use desktop_kit::image::encode_png;

/// Rectangle of `display` (index into the info's list) or of all displays together.
fn region(displays: &[Display], display: Option<u32>) -> Result<(i32, i32, u32, u32), String> {
    let picked: Vec<&Display> = match display {
        Some(i) => vec![displays.get(i as usize).ok_or("no such display")?],
        None => displays.iter().collect(),
    };
    if picked.is_empty() {
        return Err("the target reported no displays".into());
    }
    let x0 = picked.iter().map(|d| d.x).min().unwrap();
    let y0 = picked.iter().map(|d| d.y).min().unwrap();
    let x1 = picked.iter().map(|d| d.x + d.width as i32).max().unwrap();
    let y1 = picked.iter().map(|d| d.y + d.height as i32).max().unwrap();
    Ok((x0, y0, (x1 - x0) as u32, (y1 - y0) as u32))
}

/// A PNG of the screen, or why it cannot be taken.
pub fn screenshot(display: Option<u32>) -> Result<Vec<u8>, String> {
    let info = crate::info::read();
    if info.session == SessionState::Locked {
        return Err("the target is locked".into());
    }
    let (x, y, width, height) = region(&info.displays, display)?;
    let frame = grab(Rect {
        x,
        y,
        width,
        height,
    })
    .map_err(|e| e.to_string())?;
    encode_png(&frame).map_err(|e| e.to_string())
}
