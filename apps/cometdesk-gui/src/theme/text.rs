//! Typography helpers, font family selectors, captions, field labels, and shortcut chips.
//!
//! Entry points: `bold_family`, `sign`, `bold`, `mono`, `kbd`, `caption`, `field_label`.

use egui::{CornerRadius, FontFamily, FontId, Pos2, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::{BORDER, CARD_HOVER, MUTED};

/// The bold font family (Inter SemiBold).
pub fn bold_family() -> FontFamily {
    FontFamily::Name("bold".into())
}

/// Display text for names and titles: the UI face, larger, using the bold family.
#[allow(dead_code)]
pub fn sign(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::new(size, bold_family()))
}

/// Bold text in the Inter-SemiBold face.
#[allow(dead_code)]
pub fn bold(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::new(size, bold_family()))
}

/// Monospace text in JetBrains Mono.
pub fn mono(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::new(size, FontFamily::Monospace))
}

/// A small keyboard shortcut chip with JetBrains Mono 11 text.
#[allow(dead_code)]
pub fn kbd(ui: &mut Ui, text: &str) {
    let font_id = FontId::new(11.0, FontFamily::Monospace);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font_id, MUTED);
    let width = galley.size().x + 12.0;
    let height = galley.size().y + 4.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());

    if ui.is_rect_visible(rect) {
        ui.painter().rect(
            rect,
            CornerRadius::same(4),
            CARD_HOVER,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
        let text_pos = Pos2::new(rect.min.x + 6.0, rect.min.y + 2.0);
        ui.painter().galley(text_pos, galley, MUTED);
    }
}

/// A small muted section caption.
pub fn caption(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(FontId::new(11.5, bold_family()))
            .color(MUTED),
    );
}

/// A field label with a small gap beneath it.
pub fn field_label(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(FontId::new(12.0, bold_family()))
            .color(MUTED),
    );
    ui.add_space(4.0);
}
