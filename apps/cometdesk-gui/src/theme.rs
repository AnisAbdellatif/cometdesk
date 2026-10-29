//! Look of the app: palette, egui style, the logo, and small shared widgets.

use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle, Vec2};

pub const BG: Color32 = Color32::from_rgb(0x0D, 0x11, 0x1F);
pub const PANEL: Color32 = Color32::from_rgb(0x12, 0x17, 0x28);
pub const CARD: Color32 = Color32::from_rgb(0x18, 0x1E, 0x33);
pub const CARD_HOVER: Color32 = Color32::from_rgb(0x1E, 0x26, 0x40);
pub const BORDER: Color32 = Color32::from_rgb(0x2A, 0x33, 0x52);
pub const TEXT: Color32 = Color32::from_rgb(0xE6, 0xE9, 0xF5);
pub const MUTED: Color32 = Color32::from_rgb(0x8A, 0x93, 0xB2);
pub const ACCENT: Color32 = Color32::from_rgb(0x5B, 0x8C, 0xFF);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0x74, 0x9D, 0xFF);
pub const OK: Color32 = Color32::from_rgb(0x4F, 0xE3, 0xA8);
pub const WARN: Color32 = Color32::from_rgb(0xF2, 0xB8, 0x3C);
pub const ERR: Color32 = Color32::from_rgb(0xFF, 0x6B, 0x6B);

pub const LOGO_PNG: &[u8] = include_bytes!("../assets/logo-256.png");

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
        v.window_corner_radius = CornerRadius::same(14);
        v.menu_corner_radius = CornerRadius::same(10);
        v.selection.bg_fill = ACCENT.gamma_multiply(0.45);
        v.selection.stroke = Stroke::new(1.0, ACCENT_HOVER);
        v.warn_fg_color = WARN;
        v.error_fg_color = ERR;

        let r = CornerRadius::same(8);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = CARD;
        w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        w.inactive.bg_fill = CARD;
        w.inactive.weak_bg_fill = CARD;
        w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        w.hovered.bg_fill = CARD_HOVER;
        w.hovered.weak_bg_fill = CARD_HOVER;
        w.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
        w.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);
        w.active.bg_fill = ACCENT;
        w.active.weak_bg_fill = ACCENT;
        w.active.bg_stroke = Stroke::new(1.0, ACCENT_HOVER);
        w.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);
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
    egui::Button::new(egui::RichText::new(text).strong().color(Color32::WHITE))
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
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(16, 14))
}

/// A tinted notice banner.
pub fn banner(ui: &mut egui::Ui, color: Color32, text: &str) {
    egui::Frame::NONE
        .fill(color.gamma_multiply(0.14))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.5)))
        .corner_radius(CornerRadius::same(10))
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
        .corner_radius(CornerRadius::same(255))
        .inner_margin(Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, color);
                ui.label(egui::RichText::new(text).color(color).small());
            });
        });
}
