//! Look of the app: palette, egui style, the logo, and small shared widgets.
//!
//! Entry point: `apply`. Re-exports all submodules so `crate::theme::*` paths resolve directly.

mod buttons;
mod colors;
mod controls;
mod dropdown;
mod frames;
mod icons;
mod logo;
mod text;

pub use buttons::*;
pub use colors::*;
pub use controls::*;
pub use dropdown::*;
pub use frames::*;
pub use icons::*;
pub use logo::*;
pub use text::*;

use egui::{CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle, Vec2};

const INTER_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.otf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-SemiBold.otf");
const JETBRAINS_MONO: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");

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
