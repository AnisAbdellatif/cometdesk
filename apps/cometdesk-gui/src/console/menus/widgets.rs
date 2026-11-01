//! Shared popup menu frame, separator and button item widgets for toolbar and panel menus.
//! Entry points: `show_dropdown_menu`, `menu_separator`, `menu_button_item`, `menu_button_item_enabled`.

use crate::theme;

pub(in crate::console) fn show_dropdown_menu(
    ui: &mut egui::Ui,
    id: egui::Id,
    label: &str,
    width: f32,
    content: impl FnOnce(&mut egui::Ui),
) {
    let is_open = egui::Popup::is_id_open(ui.ctx(), id);
    let resp = theme::dropdown_button(ui, label, is_open);
    let frame = egui::Frame::NONE
        .fill(theme::CARD)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::same(6));
    egui::Popup::menu(&resp).id(id).frame(frame).show(|ui| {
        ui.set_width(width);
        let style = ui.style_mut();
        style.spacing.item_spacing = egui::vec2(0.0, 1.0);
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        style.visuals.widgets.hovered.weak_bg_fill = theme::CARD_HOVER;
        style.visuals.widgets.hovered.bg_fill = theme::CARD_HOVER;
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
        content(ui);
    });
}

pub(in crate::console) fn menu_separator(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::ZERO, theme::BORDER);
    ui.add_space(4.0);
}

pub(in crate::console) fn menu_button_item(
    ui: &mut egui::Ui,
    label: &str,
    note: Option<&str>,
) -> egui::Response {
    let height = 28.0;
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        if resp.hovered() {
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(6), theme::CARD_HOVER);
        }
        let font_id = egui::FontId::new(13.0, egui::FontFamily::Proportional);
        let label_galley = ui
            .painter()
            .layout_no_wrap(label.to_owned(), font_id, theme::TEXT);
        let label_pos = egui::pos2(
            rect.min.x + 10.0,
            rect.center().y - label_galley.size().y * 0.5,
        );
        ui.painter().galley(label_pos, label_galley, theme::TEXT);

        if let Some(note_text) = note {
            let note_font = egui::FontId::new(12.0, egui::FontFamily::Proportional);
            let note_galley =
                ui.painter()
                    .layout_no_wrap(note_text.to_owned(), note_font, theme::MUTED);
            let note_pos = egui::pos2(
                rect.max.x - 10.0 - note_galley.size().x,
                rect.center().y - note_galley.size().y * 0.5,
            );
            ui.painter().galley(note_pos, note_galley, theme::MUTED);
        }
    }
    resp
}

pub(in crate::console) fn menu_button_item_enabled(
    ui: &mut egui::Ui,
    enabled: bool,
    label: &str,
    tooltip: &str,
    disabled_reason: &str,
) -> egui::Response {
    let height = 28.0;
    let width = ui.available_width();
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), sense);
    if ui.is_rect_visible(rect) {
        if enabled && resp.hovered() {
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(6), theme::CARD_HOVER);
        }
        let font_id = egui::FontId::new(13.0, egui::FontFamily::Proportional);
        let color = if enabled { theme::TEXT } else { theme::MUTED };
        let label_galley = ui
            .painter()
            .layout_no_wrap(label.to_owned(), font_id, color);
        let label_pos = egui::pos2(
            rect.min.x + 10.0,
            rect.center().y - label_galley.size().y * 0.5,
        );
        ui.painter().galley(label_pos, label_galley, color);
    }
    let tip = if enabled { tooltip } else { disabled_reason };
    if !tip.is_empty() {
        resp.on_hover_text(tip)
    } else {
        resp
    }
}
