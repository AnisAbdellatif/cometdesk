//! Button widgets for primary/secondary actions, toolbar items, and vector-icon buttons.
//!
//! Entry points: `primary`, `ghost`, `icon_button`, `card_button`, `secondary_button`, `yellow_button`.

use egui::{
    Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    Vec2,
};

use super::{
    ACCENT, ACCENT_HOVER, BG, BORDER, CARD, CARD_HOVER, ChevronDir, Icon, MUTED, TEXT, bold_family,
    chevron, draw_icon,
};

/// Primary call-to-action button.
pub fn primary(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).strong().color(BG))
        .fill(ACCENT)
        .stroke(Stroke::NONE)
}

/// Quiet button without a frame until hovered.
pub fn ghost(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text).color(MUTED)).frame_when_inactive(false)
}

/// A 32x32 button that renders a drawn vector icon.
#[allow(dead_code)]
pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if resp.hovered() {
            CARD_HOVER
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect(
            rect,
            CornerRadius::same(6),
            fill,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
        let icon_color = match icon {
            Icon::More if !resp.hovered() => MUTED,
            _ => TEXT,
        };
        draw_icon(ui.painter(), rect, icon, icon_color);
    }
    if tooltip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tooltip)
    }
}

/// A card-level button (e.g. "Send file…", "Browse files…") with bold text.
pub fn card_button(ui: &mut Ui, text: &str, height: f32) -> Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if resp.hovered() { CARD_HOVER } else { CARD };
        ui.painter().rect(
            rect,
            CornerRadius::same(8),
            fill,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
        let font_id = FontId::new(13.0, bold_family());
        let galley = ui.painter().layout_no_wrap(text.to_owned(), font_id, TEXT);
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, TEXT);
    }
    resp
}

/// A secondary button (e.g. Session actions, "Set up target again…").
pub fn secondary_button(ui: &mut Ui, text: &str, height: f32, bold: bool) -> Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if resp.hovered() { BORDER } else { CARD_HOVER };
        ui.painter().rect(
            rect,
            CornerRadius::same(8),
            fill,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
        let font_id = if bold {
            FontId::new(13.0, bold_family())
        } else {
            FontId::new(13.0, FontFamily::Proportional)
        };
        let galley = ui.painter().layout_no_wrap(text.to_owned(), font_id, TEXT);
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, TEXT);
    }
    resp
}

/// A full-width signage yellow primary button (e.g. "Apply…").
pub fn yellow_button(ui: &mut Ui, text: &str, height: f32) -> Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if resp.hovered() { ACCENT_HOVER } else { ACCENT };
        ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
        let font_id = FontId::new(14.0, bold_family());
        let galley = ui.painter().layout_no_wrap(text.to_owned(), font_id, BG);
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, BG);
    }
    resp
}

/// A 32px height back button with a drawn left chevron and bold text, transparent until hovered.
#[allow(dead_code)]
pub fn back_button(ui: &mut Ui, text: &str) -> Response {
    let font_id = FontId::new(13.0, bold_family());
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font_id, TEXT);
    let total_width = 8.0 + 8.0 + 6.0 + galley.size().x + 12.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(total_width, 32.0), Sense::click());

    if ui.is_rect_visible(rect) {
        if resp.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), CARD_HOVER);
        }
        let chevron_center = Pos2::new(rect.min.x + 8.0 + 4.0, rect.center().y);
        chevron(ui.painter(), chevron_center, 4.0, ChevronDir::Left, TEXT);

        let label_pos = Pos2::new(
            rect.min.x + 8.0 + 8.0 + 6.0,
            rect.center().y - galley.size().y * 0.5,
        );
        ui.painter().galley(label_pos, galley, TEXT);
    }
    resp
}

/// A 32px height plain button transparent until hovered.
pub fn plain_button(ui: &mut Ui, text: &str) -> Response {
    let font_id = FontId::new(13.0, FontFamily::Proportional);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font_id, TEXT);
    let total_width = 12.0 + galley.size().x + 12.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(total_width, 32.0), Sense::click());
    if ui.is_rect_visible(rect) {
        if resp.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), CARD_HOVER);
        }
        let pos = Pos2::new(rect.min.x + 12.0, rect.center().y - galley.size().y * 0.5);
        ui.painter().galley(pos, galley, TEXT);
    }
    resp
}

/// A 32px height Actions button with label and a drawn JetBrains Mono "Cmd+K" chip.
#[allow(dead_code)]
pub fn actions_button(ui: &mut Ui) -> Response {
    let font_id = FontId::new(13.0, FontFamily::Proportional);
    let label_galley = ui
        .painter()
        .layout_no_wrap("Actions".to_owned(), font_id, MUTED);
    let kbd_font = FontId::new(11.0, FontFamily::Monospace);
    let kbd_galley = ui
        .painter()
        .layout_no_wrap("Cmd+K".to_owned(), kbd_font, TEXT);

    let kbd_w = kbd_galley.size().x + 12.0;
    let kbd_h = kbd_galley.size().y + 4.0;
    let total_w = 12.0 + label_galley.size().x + 8.0 + kbd_w + 6.0;

    let (rect, resp) = ui.allocate_exact_size(Vec2::new(total_w, 32.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if resp.hovered() { CARD_HOVER } else { CARD };
        ui.painter().rect(
            rect,
            CornerRadius::same(6),
            fill,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
        let label_pos = Pos2::new(
            rect.min.x + 12.0,
            rect.center().y - label_galley.size().y * 0.5,
        );
        ui.painter().galley(
            label_pos,
            label_galley,
            if resp.hovered() { TEXT } else { MUTED },
        );

        let kbd_rect = Rect::from_center_size(
            Pos2::new(rect.max.x - 6.0 - kbd_w * 0.5, rect.center().y),
            Vec2::new(kbd_w, kbd_h),
        );
        ui.painter().rect(
            kbd_rect,
            CornerRadius::same(4),
            CARD_HOVER,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
        let kbd_pos = Pos2::new(
            kbd_rect.min.x + 6.0,
            kbd_rect.center().y - kbd_galley.size().y * 0.5,
        );
        ui.painter().galley(kbd_pos, kbd_galley, TEXT);
    }
    resp
}

/// A 32px height toggle button (like Panel) showing selected state when open.
#[allow(dead_code)]
pub fn toggle_button(ui: &mut Ui, text: &str, active: bool) -> Response {
    let font_id = FontId::new(13.0, bold_family());
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font_id, TEXT);
    let total_width = 12.0 + galley.size().x + 12.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(total_width, 32.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if active || resp.hovered() {
            CARD_HOVER
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect(
            rect,
            CornerRadius::same(6),
            fill,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
        let pos = Pos2::new(rect.min.x + 12.0, rect.center().y - galley.size().y * 0.5);
        ui.painter().galley(pos, galley, TEXT);
    }
    resp
}
