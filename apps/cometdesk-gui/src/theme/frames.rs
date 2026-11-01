//! Card frames, banners, status dots/pills/tags, collapsible sections, and dashed dividers.
//!
//! Entry points: `card`, `panel_card`, `modal_frame`, `banner`, `attention_banner`, `section`.

use egui::{
    Color32, CornerRadius, FontFamily, FontId, Margin, Painter, Pos2, Rect, Sense, Stroke,
    StrokeKind, Ui, Vec2,
};

use super::{BORDER, CARD, PANEL, TEXT, WARN, bold_family};

#[allow(deprecated)]
fn inline_id() -> egui::Id {
    egui::Id::new("inline_sections")
}

/// Inside the side panel, groups open in place instead of in a popup menu.
pub fn set_inline_sections(ui: &mut egui::Ui, on: bool) {
    ui.data_mut(|d| d.insert_temp(inline_id(), on));
}

/// A group of settings: a popup menu in the toolbar, a collapsible section in the panel.
pub fn section(
    ui: &mut egui::Ui,
    title: impl Into<egui::WidgetText>,
    add: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    if ui
        .data(|d| d.get_temp::<bool>(inline_id()))
        .unwrap_or(false)
    {
        egui::CollapsingHeader::new(title)
            .show(ui, add)
            .header_response
    } else {
        ui.menu_button(title, add).response
    }
}

/// Status as a dot and a word, with no container.
pub fn status_dot(ui: &mut egui::Ui, color: Color32, text: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.0, color);
        ui.label(egui::RichText::new(text).color(color));
    });
}

/// A card frame.
#[allow(dead_code)]
pub fn card() -> egui::Frame {
    egui::Frame::NONE
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(16, 14))
}

/// A tinted notice banner.
pub fn banner(ui: &mut egui::Ui, color: Color32, text: &str) {
    egui::Frame::NONE
        .fill(color.gamma_multiply(0.14))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.5)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.colored_label(color, text);
        });
}

/// A status dot followed by text, in a rounded pill.
pub fn status_pill(ui: &mut egui::Ui, color: Color32, text: &str) {
    egui::Frame::NONE
        .fill(color.gamma_multiply(0.14))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, color);
                ui.label(egui::RichText::new(text).color(color).small());
            });
        });
}

/// A status pill on an opaque surface, for placing over the video.
pub fn floating_status(ui: &mut egui::Ui, color: Color32, text: &str) {
    egui::Frame::NONE
        .fill(PANEL)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(4))
        .show(ui, |ui| status_pill(ui, color, text));
}

/// A panel card frame with 10px radius and 12px inner margin.
pub fn panel_card() -> egui::Frame {
    egui::Frame::NONE
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(12))
}

/// An amber attention banner for the side panel.
pub fn attention_banner(ui: &mut Ui, text: &str) {
    let width = ui.available_width();
    let font_id = FontId::new(13.0, FontFamily::Proportional);
    let galley = ui
        .painter()
        .layout(text.to_owned(), font_id, TEXT, (width - 44.0).max(100.0));
    let height = (galley.size().y + 20.0).max(38.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    if ui.is_rect_visible(rect) {
        let bg = Color32::from_rgb(0x2A, 0x1D, 0x0C);
        ui.painter().rect(
            rect,
            CornerRadius::same(8),
            bg,
            Stroke::new(1.0, WARN),
            StrokeKind::Inside,
        );
        let dot_pos = Pos2::new(rect.min.x + 12.0 + 4.0, rect.min.y + 14.0);
        ui.painter().circle_filled(dot_pos, 4.0, WARN);
        let text_pos = Pos2::new(rect.min.x + 12.0 + 8.0 + 10.0, rect.min.y + 10.0);
        ui.painter().galley(text_pos, galley, TEXT);
    }
}

/// A status or indicator pill tag.
pub fn tag(ui: &mut Ui, text: &str, color: Color32, bg: Color32) {
    let font_id = FontId::new(12.0, bold_family());
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font_id, color);
    let width = galley.size().x + 16.0;
    let height = 20.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, CornerRadius::same(10), bg);
        let pos = Pos2::new(rect.min.x + 8.0, rect.center().y - galley.size().y * 0.5);
        ui.painter().galley(pos, galley, color);
    }
}

/// A modal dialog frame with 12px radius and 24px inner margin.
#[allow(dead_code)]
pub fn modal_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(PANEL)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(24))
}

/// Draw a dashed rounded rectangle outline.
#[allow(dead_code)]
pub fn draw_dashed_rect(
    painter: &Painter,
    rect: Rect,
    radius: f32,
    stroke: Stroke,
    dash_len: f32,
    gap_len: f32,
) {
    let r = radius.min(rect.width() * 0.5).min(rect.height() * 0.5);
    let w = rect.width();
    let h = rect.height();
    let straight_w = w - 2.0 * r;
    let straight_h = h - 2.0 * r;
    let arc_len = std::f32::consts::FRAC_PI_2 * r;
    let perimeter = 2.0 * straight_w + 2.0 * straight_h + 4.0 * arc_len;
    if perimeter <= 0.0 {
        return;
    }

    let point_at = |mut s: f32| -> Pos2 {
        s = s.rem_euclid(perimeter);
        if s < straight_w {
            return Pos2::new(rect.min.x + r + s, rect.min.y);
        }
        s -= straight_w;
        if s < arc_len {
            let theta = -std::f32::consts::FRAC_PI_2 + (s / arc_len) * std::f32::consts::FRAC_PI_2;
            return Pos2::new(
                rect.max.x - r + r * theta.cos(),
                rect.min.y + r + r * theta.sin(),
            );
        }
        s -= arc_len;
        if s < straight_h {
            return Pos2::new(rect.max.x, rect.min.y + r + s);
        }
        s -= straight_h;
        if s < arc_len {
            let theta = (s / arc_len) * std::f32::consts::FRAC_PI_2;
            return Pos2::new(
                rect.max.x - r + r * theta.cos(),
                rect.max.y - r + r * theta.sin(),
            );
        }
        s -= arc_len;
        if s < straight_w {
            return Pos2::new(rect.max.x - r - s, rect.max.y);
        }
        s -= straight_w;
        if s < arc_len {
            let theta = std::f32::consts::FRAC_PI_2 + (s / arc_len) * std::f32::consts::FRAC_PI_2;
            return Pos2::new(
                rect.min.x + r + r * theta.cos(),
                rect.max.y - r + r * theta.sin(),
            );
        }
        s -= arc_len;
        if s < straight_h {
            return Pos2::new(rect.min.x, rect.max.y - r - s);
        }
        s -= straight_h;
        let theta = std::f32::consts::PI + (s / arc_len) * std::f32::consts::FRAC_PI_2;
        Pos2::new(
            rect.min.x + r + r * theta.cos(),
            rect.min.y + r + r * theta.sin(),
        )
    };

    let step = dash_len + gap_len;
    let mut s = 0.0;
    while s < perimeter {
        let p_start = point_at(s);
        let end_s = (s + dash_len).min(perimeter);
        let mid_s = s + (end_s - s) * 0.5;
        let p_mid = point_at(mid_s);
        let p_end = point_at(end_s);
        painter.line_segment([p_start, p_mid], stroke);
        painter.line_segment([p_mid, p_end], stroke);
        s += step;
    }
}

/// A 1px wide vertical divider of the given height with BORDER stroke.
#[allow(dead_code)]
pub fn vdivider(ui: &mut Ui, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, height), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, BORDER);
}
