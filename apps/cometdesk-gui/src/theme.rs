//! Look of the app: palette, egui style, the logo, and small shared widgets.

use egui::{
    Align2, Color32, CornerRadius, FontFamily, FontId, Id, Margin, Painter, Pos2, Rect, Response,
    RichText, Sense, Stroke, StrokeKind, TextStyle, Ui, Vec2,
};

const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.otf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.otf");
const JETBRAINS_MONO: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");

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

pub const LOGO_PNG: &[u8] = include_bytes!("../assets/logo-256.png");

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

pub fn apply(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "inter".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(INTER_REGULAR)),
    );
    fonts.font_data.insert(
        "inter-bold".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(INTER_SEMIBOLD)),
    );
    fonts.font_data.insert(
        "jbmono".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(JETBRAINS_MONO)),
    );

    let prop_defaults = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut prop = vec!["inter".to_owned()];
    prop.extend(prop_defaults.clone());
    fonts.families.insert(FontFamily::Proportional, prop);

    let mut bold_family_fonts = vec!["inter-bold".to_owned()];
    bold_family_fonts.extend(prop_defaults);
    fonts.families.insert(bold_family(), bold_family_fonts);

    let mono_defaults = fonts
        .families
        .get(&FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();
    let mut mono = vec!["jbmono".to_owned()];
    mono.extend(mono_defaults);
    fonts.families.insert(FontFamily::Monospace, mono);

    ctx.set_fonts(fonts);

    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        let v = &mut style.visuals;
        v.dark_mode = true;
        v.override_text_color = Some(TEXT);
        v.panel_fill = PANEL;
        v.window_fill = CARD;
        v.extreme_bg_color = BG;
        v.faint_bg_color = CARD;
        v.hyperlink_color = ACCENT;
        v.window_stroke = Stroke::new(1.0, BORDER);
        v.window_corner_radius = CornerRadius::same(12);
        v.menu_corner_radius = CornerRadius::same(8);
        v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
        v.selection.stroke = Stroke::new(1.0, ACCENT_HOVER);
        v.warn_fg_color = WARN;
        v.error_fg_color = ERR;

        let r = CornerRadius::same(6);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = CARD;
        w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        w.inactive.bg_fill = CARD;
        w.inactive.weak_bg_fill = CARD;
        w.inactive.bg_stroke = Stroke::new(1.0, FIELD_RULE);
        w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        w.hovered.bg_fill = CARD_HOVER;
        w.hovered.weak_bg_fill = CARD_HOVER;
        w.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
        w.hovered.fg_stroke = Stroke::new(1.5, TEXT);
        w.active.bg_fill = ACCENT;
        w.active.weak_bg_fill = ACCENT;
        w.active.bg_stroke = Stroke::new(1.0, ACCENT_HOVER);
        // egui draws `strong()` text with this colour, so it must stay light.
        w.active.fg_stroke = Stroke::new(1.5, TEXT);
        w.open.bg_fill = CARD_HOVER;
        w.open.weak_bg_fill = CARD_HOVER;
        w.open.bg_stroke = Stroke::new(1.0, ACCENT);
        for s in [
            &mut w.noninteractive,
            &mut w.inactive,
            &mut w.hovered,
            &mut w.active,
            &mut w.open,
        ] {
            s.corner_radius = r;
        }

        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
        style.spacing.interact_size.y = 32.0;
        style.spacing.menu_margin = Margin::same(8);
        style.text_styles = [
            (TextStyle::Heading, FontId::new(20.0, bold_family())),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(13.5, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(12.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(12.5, FontFamily::Monospace),
            ),
        ]
        .into();
    });
}

/// The logo as a texture (decoded from the embedded PNG).
pub fn logo_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(LOGO_PNG))
        .read_info()
        .ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [info.width as usize, info.height as usize],
        &buf[..info.buffer_size()],
    );
    Some(ctx.load_texture("logo", image, egui::TextureOptions::LINEAR))
}

/// RGBA pixels of the logo, for the window icon.
#[cfg(not(target_os = "macos"))]
pub fn logo_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(LOGO_PNG))
        .read_info()
        .ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    Some((buf, info.width, info.height))
}

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

/// Icons drawn via vector primitives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Icon {
    Plus,
    Pencil,
    Trash,
    Fullscreen,
    ExitFullscreen,
    Search,
    Check,
    More,
}

/// Draw a vector icon centered within a rectangle using 1.6px strokes.
#[allow(dead_code)]
pub fn draw_icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let pt = |dx: f32, dy: f32| Pos2::new(c.x + dx, c.y + dy);
    let stroke = Stroke::new(1.6, color);

    match icon {
        Icon::Plus => {
            painter.line_segment([pt(-6.0, 0.0), pt(6.0, 0.0)], stroke);
            painter.line_segment([pt(0.0, -6.0), pt(0.0, 6.0)], stroke);
        }
        Icon::Pencil => {
            let pts = vec![
                pt(-5.0, 5.0),
                pt(-4.0, 1.5),
                pt(2.5, -5.0),
                pt(5.0, -2.5),
                pt(-1.5, 4.0),
                pt(-5.0, 5.0),
            ];
            painter.add(egui::Shape::line(pts, stroke));
            painter.line_segment([pt(-4.0, 1.5), pt(-1.5, 4.0)], stroke);
        }
        Icon::Trash => {
            painter.line_segment([pt(-5.0, -3.5), pt(5.0, -3.5)], stroke);
            painter.add(egui::Shape::line(
                vec![pt(-1.5, -3.5), pt(-1.5, -5.0), pt(1.5, -5.0), pt(1.5, -3.5)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(-3.5, -3.5), pt(-2.5, 5.5), pt(2.5, 5.5), pt(3.5, -3.5)],
                stroke,
            ));
            painter.line_segment([pt(-1.0, -1.5), pt(-0.8, 3.5)], stroke);
            painter.line_segment([pt(1.0, -1.5), pt(0.8, 3.5)], stroke);
        }
        Icon::Fullscreen => {
            painter.add(egui::Shape::line(
                vec![pt(-6.0, -2.5), pt(-6.0, -6.0), pt(-2.5, -6.0)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(2.5, -6.0), pt(6.0, -6.0), pt(6.0, -2.5)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(6.0, 2.5), pt(6.0, 6.0), pt(2.5, 6.0)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(-2.5, 6.0), pt(-6.0, 6.0), pt(-6.0, 2.5)],
                stroke,
            ));
        }
        Icon::ExitFullscreen => {
            painter.add(egui::Shape::line(
                vec![pt(-6.0, -2.5), pt(-2.5, -2.5), pt(-2.5, -6.0)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(6.0, -2.5), pt(2.5, -2.5), pt(2.5, -6.0)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(6.0, 2.5), pt(2.5, 2.5), pt(2.5, 6.0)],
                stroke,
            ));
            painter.add(egui::Shape::line(
                vec![pt(-6.0, 2.5), pt(-2.5, 2.5), pt(-2.5, 6.0)],
                stroke,
            ));
        }
        Icon::Search => {
            painter.circle_stroke(pt(-1.5, -1.5), 4.5, stroke);
            painter.line_segment([pt(1.8, 1.8), pt(5.5, 5.5)], stroke);
        }
        Icon::Check => {
            painter.add(egui::Shape::line(
                vec![pt(-5.0, 0.0), pt(-1.5, 4.0), pt(5.5, -3.5)],
                stroke,
            ));
        }
        Icon::More => {
            painter.circle_filled(pt(-4.5, 0.0), 1.5, color);
            painter.circle_filled(pt(0.0, 0.0), 1.5, color);
            painter.circle_filled(pt(4.5, 0.0), 1.5, color);
        }
    }
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
            Icon::More => MUTED,
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
