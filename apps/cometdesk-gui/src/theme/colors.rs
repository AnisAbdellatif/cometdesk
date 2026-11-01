//! Color palette constants, device identity line colors, and initial badges.
//!
//! Entry points: `line_color`, `badge`, palette `Color32` constants.

use egui::{Color32, CornerRadius, FontId, Vec2};

use super::bold_family;

/// Outline of inputs and buttons at rest: lighter than `BORDER` so fields read on cards.
pub const FIELD_RULE: Color32 = Color32::from_rgb(0x6B, 0x69, 0x57);
pub const BG: Color32 = Color32::from_rgb(0x0B, 0x0B, 0x09);
pub const PANEL: Color32 = Color32::from_rgb(0x12, 0x12, 0x0F);
pub const CARD: Color32 = Color32::from_rgb(0x1A, 0x1A, 0x15);
pub const CARD_HOVER: Color32 = Color32::from_rgb(0x24, 0x24, 0x1C);
pub const WELL: Color32 = Color32::from_rgb(0x0F, 0x0F, 0x0C);
pub const BORDER: Color32 = Color32::from_rgb(0x3A, 0x39, 0x2E);
pub const TEXT: Color32 = Color32::from_rgb(0xF4, 0xF1, 0xE3);
pub const MUTED: Color32 = Color32::from_rgb(0xA8, 0xA4, 0x8C);
/// Signage yellow: the primary action, focus and selection. Text on it is `BG`.
pub const ACCENT: Color32 = Color32::from_rgb(0xFF, 0xD2, 0x1F);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0xFF, 0xE2, 0x62);
pub const OK: Color32 = Color32::from_rgb(0x3F, 0xD3, 0x8A);
pub const WARN: Color32 = Color32::from_rgb(0xFF, 0x8F, 0x2B);
pub const ERR: Color32 = Color32::from_rgb(0xFF, 0x5A, 0x52);

/// Identity colours, one per device: its "line". None is a status or accent colour.
const LINES: [Color32; 6] = [
    Color32::from_rgb(0x3F, 0xA7, 0xFF),
    Color32::from_rgb(0xA7, 0x7B, 0xFF),
    Color32::from_rgb(0x19, 0xC2, 0xC2),
    Color32::from_rgb(0xF2, 0x5F, 0xA8),
    Color32::from_rgb(0x9B, 0xD5, 0x2E),
    Color32::from_rgb(0xD9, 0xB3, 0x8C),
];

/// The line colour of the device with this key (its host): stable across runs.
pub fn line_color(key: &str) -> Color32 {
    let h = key
        .bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33) ^ u32::from(b));
    LINES[h as usize % LINES.len()]
}

/// A square badge in the device's line colour carrying its initial.
pub fn badge(ui: &mut egui::Ui, key: &str, name: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let letter = name
        .chars()
        .find(|c| c.is_alphanumeric())
        .map_or('?', |c| c.to_ascii_uppercase());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(10), line_color(key));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        letter,
        FontId::new(size * 0.55, bold_family()),
        BG,
    );
}
