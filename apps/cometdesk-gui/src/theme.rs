//! Look of the app: palette, egui style, the logo, and small shared widgets.

use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle, Vec2};

/// Outline of inputs and buttons at rest: lighter than `BORDER` so fields read on cards.
pub const FIELD_RULE: Color32 = Color32::from_rgb(0x6B, 0x69, 0x57);
pub const BG: Color32 = Color32::from_rgb(0x0B, 0x0B, 0x09);
pub const PANEL: Color32 = Color32::from_rgb(0x12, 0x12, 0x0F);
pub const CARD: Color32 = Color32::from_rgb(0x1A, 0x1A, 0x15);
pub const CARD_HOVER: Color32 = Color32::from_rgb(0x24, 0x24, 0x1C);
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
        .rect_filled(rect, CornerRadius::same(4), line_color(key));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        letter,
        FontId::new(size * 0.62, FontFamily::Proportional),
        BG,
    );
}

pub const LOGO_PNG: &[u8] = include_bytes!("../assets/logo-256.png");

/// Display text for names and titles: the UI face, larger.
pub fn sign(text: impl Into<String>, size: f32) -> egui::RichText {
    egui::RichText::new(text).size(size)
}

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
        v.window_corner_radius = CornerRadius::same(8);
        v.menu_corner_radius = CornerRadius::same(6);
        v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
        v.selection.stroke = Stroke::new(1.0, ACCENT_HOVER);
        v.warn_fg_color = WARN;
        v.error_fg_color = ERR;

        let r = CornerRadius::same(4);
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
        style.spacing.interact_size.y = 28.0;
        style.spacing.menu_margin = Margin::same(8);
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(24.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(14.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(12.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(13.0, FontFamily::Monospace),
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
