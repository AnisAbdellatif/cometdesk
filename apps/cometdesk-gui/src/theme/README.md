# `theme` — Palette, typography, icons, and shared egui widgets

- `mod.rs` — Font registration, global egui dark-theme `apply`, and re-exports of all theme items.
- `colors.rs` — Palette `Color32` constants, per-device identity `line_color`, and initial `badge`.
- `text.rs` — Font family helpers (`bold_family`, `sign`, `bold`, `mono`), `kbd` chip, and labels.
- `logo.rs` — Embedded `LOGO_PNG` bytes, egui `logo_texture`, and non-macOS `logo_rgba` decoder.
- `frames.rs` — Card/modal frames, banners, status dots/pills/tags, collapsible sections, and dividers.
- `buttons.rs` — Primary, ghost, icon, card, secondary, yellow, back, plain, actions, and toggle buttons.
- `dropdown.rs` — `ChevronDir`, `chevron` polyline primitive, `dropdown_button`, and `show_dropdown_row`.
- `controls.rs` — Equal-width `segmented` control, `switch`, `labelled_switch`, and numeric `field_box`.
- `icons.rs` — `Icon` enum and vector stroke renderer `draw_icon`.
