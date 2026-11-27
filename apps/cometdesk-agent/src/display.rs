//! The target's monitors: what `Info::displays` reports, and changing a monitor's resolution
//! or making it the primary one. The OS calls live in `desktop-kit`; this converts its types.

use cometdesk_clip::Display;

fn to_clip(d: desktop_kit::display::Display) -> Display {
    Display {
        x: d.x,
        y: d.y,
        width: d.width,
        height: d.height,
        scale: d.scale,
        primary: d.primary,
        modes: d.modes,
    }
}

/// The monitors in the order `Info::displays` has them. `scale` is the desktop's scale in
/// percent where the OS gives no per-monitor value.
pub fn list(scale: u32) -> Vec<Display> {
    desktop_kit::display::list(scale)
        .into_iter()
        .map(to_clip)
        .collect()
}

/// Changes the resolution of display `index` to `mode`, and/or makes it the primary one.
pub fn set(index: u32, mode: Option<(u32, u32)>, primary: bool) -> Result<(), String> {
    desktop_kit::display::set(index, mode, primary).map_err(|e| e.to_string())
}
