//! Chevron primitive, toolbar dropdown buttons, and side-panel dropdown popup rows.
//!
//! Entry points: `ChevronDir`, `chevron`, `dropdown_button`, `dropdown_row`, `show_dropdown_row`.

use egui::{
    Color32, CornerRadius, FontFamily, FontId, Id, Margin, Painter, Pos2, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2,
};

use super::{BORDER, CARD, CARD_HOVER, MUTED, TEXT, WELL};

/// Direction for a drawn chevron.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChevronDir {
    Down,
    Up,
    Left,
    Right,
}

/// A 2px round-capped polyline chevron.
pub fn chevron(painter: &Painter, center: Pos2, size: f32, dir: ChevronDir, color: Color32) {
    let stroke = Stroke::new(2.0, color);
    let (p1, p2, p3) = match dir {
        ChevronDir::Down => (
            Pos2::new(center.x - size, center.y - size * 0.5),
            Pos2::new(center.x, center.y + size * 0.5),
            Pos2::new(center.x + size, center.y - size * 0.5),
        ),
        ChevronDir::Up => (
            Pos2::new(center.x - size, center.y + size * 0.5),
            Pos2::new(center.x, center.y - size * 0.5),
            Pos2::new(center.x + size, center.y + size * 0.5),
        ),
        ChevronDir::Left => (
            Pos2::new(center.x + size * 0.5, center.y - size),
            Pos2::new(center.x - size * 0.5, center.y),
            Pos2::new(center.x + size * 0.5, center.y + size),
        ),
        ChevronDir::Right => (
            Pos2::new(center.x - size * 0.5, center.y - size),
            Pos2::new(center.x + size * 0.5, center.y),
            Pos2::new(center.x - size * 0.5, center.y + size),
        ),
    };
    painter.add(egui::Shape::line(vec![p1, p2, p3], stroke));
}

/// A height-32 button displaying a label and a drawn chevron for opening dropdown menus.
#[allow(dead_code)]
pub fn dropdown_button(ui: &mut Ui, label: &str, open: bool) -> Response {
    let font_id = FontId::new(13.5, FontFamily::Proportional);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font_id, TEXT);
    let label_width = galley.size().x;
    let total_width = 12.0 + label_width + 8.0 + 10.0 + 10.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(total_width, 32.0), Sense::click());

    if ui.is_rect_visible(rect) {
        let fill = if open || resp.hovered() {
            CARD_HOVER
        } else {
            WELL
        };
        ui.painter().rect(
            rect,
            CornerRadius::same(6),
            fill,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );

        let label_pos = Pos2::new(rect.min.x + 12.0, rect.center().y - galley.size().y * 0.5);
        ui.painter().galley(label_pos, galley, TEXT);

        let chevron_center =
            Pos2::new(rect.min.x + 12.0 + label_width + 8.0 + 5.0, rect.center().y);
        let chevron_dir = if open {
            ChevronDir::Up
        } else {
            ChevronDir::Down
        };
        let chevron_color = if open || resp.hovered() { TEXT } else { MUTED };
        chevron(
            ui.painter(),
            chevron_center,
            4.0,
            chevron_dir,
            chevron_color,
        );
    }

    resp
}

/// A 36px dropdown row for panel cards with label on the left and value + chevron on the right.
pub fn dropdown_row(ui: &mut Ui, label: &str, value: &str, open: bool) -> Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 36.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let fill = if open || resp.hovered() {
            CARD_HOVER
        } else {
            CARD
        };
        ui.painter().rect(
            rect,
            CornerRadius::same(8),
            fill,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
        let font_id = FontId::new(13.0, FontFamily::Proportional);
        let label_galley = ui
            .painter()
            .layout_no_wrap(label.to_owned(), font_id.clone(), MUTED);
        let label_w = label_galley.size().x;
        let label_pos = Pos2::new(
            rect.min.x + 12.0,
            rect.center().y - label_galley.size().y * 0.5,
        );
        ui.painter().galley(label_pos, label_galley, MUTED);

        let chevron_color = if open || resp.hovered() { TEXT } else { MUTED };
        let chevron_center = Pos2::new(rect.max.x - 12.0 - 4.0, rect.center().y);
        let chevron_dir = if open {
            ChevronDir::Up
        } else {
            ChevronDir::Down
        };
        chevron(
            ui.painter(),
            chevron_center,
            4.0,
            chevron_dir,
            chevron_color,
        );

        // Long values are cut with an ellipsis rather than running into the label.
        let value_w = (rect.width() - 12.0 - label_w - 12.0 - 12.0 - 12.0).max(24.0);
        let mut job = egui::text::LayoutJob::simple_singleline(value.to_owned(), font_id, TEXT);
        job.wrap = egui::text::TextWrapping::truncate_at_width(value_w);
        let val_galley = ui.painter().layout_job(job);
        let val_pos = Pos2::new(
            rect.max.x - 12.0 - 12.0 - val_galley.size().x,
            rect.center().y - val_galley.size().y * 0.5,
        );
        ui.painter().galley(val_pos, val_galley, TEXT);
    }
    resp
}

/// Displays a dropdown row and handles opening an egui popup menu with custom content.
pub fn show_dropdown_row(
    ui: &mut Ui,
    id: Id,
    label: &str,
    value: &str,
    width: f32,
    content: impl FnOnce(&mut Ui),
) {
    let is_open = egui::Popup::is_id_open(ui.ctx(), id);
    let resp = dropdown_row(ui, label, value, is_open);
    if resp.clicked() {
        egui::Popup::toggle_id(ui.ctx(), id);
    }
    let frame = egui::Frame::NONE
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(6));
    egui::Popup::menu(&resp).id(id).frame(frame).show(|ui| {
        ui.set_width(width);
        let style = ui.style_mut();
        style.spacing.item_spacing = Vec2::new(0.0, 1.0);
        style.spacing.button_padding = Vec2::new(10.0, 4.0);
        style.visuals.widgets.hovered.weak_bg_fill = CARD_HOVER;
        style.visuals.widgets.hovered.bg_fill = CARD_HOVER;
        style.visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
        style.visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
        content(ui);
    });
}
