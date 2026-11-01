//! Interactive form controls: segmented selectors, toggle switches, and numeric drag boxes.
//!
//! Entry points: `segmented`, `switch`, `labelled_switch`, `field_box`.

use egui::{
    Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Response, RichText, Sense,
    Stroke, StrokeKind, Ui, Vec2,
};

use super::{ACCENT, BG, BORDER, MUTED, TEXT, bold_family};

/// An equal-width segmented control spanning the full available width.
pub fn segmented(ui: &mut Ui, selected: &mut usize, labels: &[&str]) -> Response {
    let width = ui.available_width();
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::click());
    let inner_rect = rect.shrink(3.0);
    let n = labels.len();

    if let (true, Some(pos)) = (n > 0 && resp.clicked(), resp.interact_pointer_pos()) {
        let rel_x = (pos.x - inner_rect.min.x).clamp(0.0, inner_rect.width());
        let seg_width = inner_rect.width() / n as f32;
        let idx = ((rel_x / seg_width).floor() as usize).min(n - 1);
        if idx != *selected {
            *selected = idx;
            resp.mark_changed();
        }
    }

    if ui.is_rect_visible(rect) {
        ui.painter().rect(
            rect,
            CornerRadius::same(8),
            BG,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
        if n > 0 {
            let seg_width = inner_rect.width() / n as f32;
            for (i, label) in labels.iter().enumerate() {
                let seg_rect = Rect::from_min_size(
                    Pos2::new(inner_rect.min.x + i as f32 * seg_width, inner_rect.min.y),
                    Vec2::new(seg_width, inner_rect.height()),
                );
                if i == *selected {
                    ui.painter()
                        .rect_filled(seg_rect, CornerRadius::same(6), BORDER);
                    ui.painter().text(
                        seg_rect.center(),
                        Align2::CENTER_CENTER,
                        *label,
                        FontId::new(12.0, bold_family()),
                        TEXT,
                    );
                } else {
                    ui.painter().text(
                        seg_rect.center(),
                        Align2::CENTER_CENTER,
                        *label,
                        FontId::new(12.0, FontFamily::Proportional),
                        MUTED,
                    );
                }
            }
        }
    }

    resp
}

/// A 36x20 toggle switch pill.
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(36.0, 20.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let bg_fill = if *on { ACCENT } else { BORDER };
        let knob_fill = if *on { BG } else { MUTED };
        let knob_x = if *on {
            rect.max.x - 10.0
        } else {
            rect.min.x + 10.0
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(10), bg_fill);
        ui.painter()
            .circle_filled(Pos2::new(knob_x, rect.center().y), 7.0, knob_fill);
    }
    resp
}

/// A row with a label on the left and a switch right-aligned.
pub fn labelled_switch(ui: &mut Ui, label: &str, on: &mut bool) -> Response {
    ui.horizontal(|ui| {
        let label_resp = ui.add(
            egui::Label::new(
                RichText::new(label)
                    .color(TEXT)
                    .font(FontId::new(13.5, FontFamily::Proportional)),
            )
            .sense(Sense::click()),
        );
        let mut resp = ui
            .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                switch(ui, on)
            })
            .inner;
        if label_resp.clicked() {
            *on = !*on;
            resp.mark_changed();
        }
        resp
    })
    .inner
}

/// An equal-width field input box with label on the left and DragValue on the right.
pub fn field_box(
    ui: &mut Ui,
    width: f32,
    label: &str,
    val: &mut i64,
    range: std::ops::RangeInclusive<i64>,
    speed: f64,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 36.0), Sense::click());
    if ui.is_rect_visible(rect) {
        ui.painter().rect(
            rect,
            CornerRadius::same(8),
            BG,
            Stroke::new(1.0, BORDER),
            StrokeKind::Inside,
        );
        let font_id = FontId::new(13.0, FontFamily::Proportional);
        let label_galley = ui
            .painter()
            .layout_no_wrap(label.to_owned(), font_id, MUTED);
        ui.painter().galley(
            Pos2::new(
                rect.min.x + 12.0,
                rect.center().y - label_galley.size().y * 0.5,
            ),
            label_galley,
            MUTED,
        );
    }
    let dv_rect = Rect::from_min_max(
        Pos2::new(rect.min.x + 45.0, rect.min.y + 2.0),
        Pos2::new(rect.max.x - 8.0, rect.max.y - 2.0),
    );
    let mut dv_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(dv_rect)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    dv_ui.style_mut().visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
    dv_ui.style_mut().visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    dv_ui.style_mut().visuals.widgets.hovered.bg_fill = Color32::TRANSPARENT;
    dv_ui.style_mut().visuals.widgets.hovered.weak_bg_fill = Color32::TRANSPARENT;
    dv_ui.style_mut().visuals.widgets.active.bg_fill = Color32::TRANSPARENT;
    dv_ui.style_mut().visuals.widgets.active.weak_bg_fill = Color32::TRANSPARENT;
    dv_ui.style_mut().visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    dv_ui.style_mut().visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    dv_ui.style_mut().visuals.widgets.active.bg_stroke = Stroke::NONE;
    let dv_resp = dv_ui.add(egui::DragValue::new(val).range(range).speed(speed));
    if dv_resp.changed() {
        return dv_resp;
    }
    resp
}
