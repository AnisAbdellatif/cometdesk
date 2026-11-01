//! Top console toolbar: device badge, status pill, Keys/Paste/Capture/Power menus and viewer controls.
//! Entry point: `Console::toolbar`.

use cometdesk::api::atx::{AtxButton, PowerAction};
use cometdesk::api::capabilities::Feature;

use crate::actions;
use crate::console::menus::widgets::{
    menu_button_item, menu_button_item_enabled, menu_separator, show_dropdown_menu,
};
use crate::console::state::{Confirm, Tab};
use crate::console::{Console, ConsoleAction, STATUS_WIDTH, debug_view};
use crate::theme;

impl Console {
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui) -> ConsoleAction {
        let mut action = ConsoleAction::None;
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);

        // Left: where am I, and is it working.
        if theme::back_button(ui, "Devices").clicked() {
            action = ConsoleAction::Disconnect;
        }
        theme::vdivider(ui, 20.0);
        theme::badge(ui, &self.device.host, self.device.display_name(), 24.0);
        ui.label(
            egui::RichText::new(self.device.display_name())
                .font(egui::FontId::new(14.0, theme::bold_family()))
                .color(theme::TEXT),
        );
        let (text, color) = self.status();
        // Fixed width: a status change must not shift the controls under the pointer.
        ui.allocate_ui_with_layout(
            egui::vec2(STATUS_WIDTH, 32.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| theme::status_dot(ui, color, &text),
        );
        theme::vdivider(ui, 20.0);

        // Do things on the target.
        let keys_id = ui.make_persistent_id("keys_menu_popup");
        #[cfg(debug_assertions)]
        if debug_view().as_deref() == Some("keys") {
            let debug_keys_id = ui.make_persistent_id("debug_keys_done");
            let debug_keys_opened = ui
                .ctx()
                .data(|d| d.get_temp::<bool>(debug_keys_id).unwrap_or(false));
            if !debug_keys_opened {
                ui.ctx().data_mut(|d| d.insert_temp(debug_keys_id, true));
                egui::Popup::open_id(ui.ctx(), keys_id);
            }
        }
        show_dropdown_menu(ui, keys_id, "Keys", 260.0, |ui| {
            let mut separated = false;
            for (label, keys) in actions::COMBOS {
                let is_os = label.contains("Linux") || label.contains("macOS");
                if is_os && !separated {
                    menu_separator(ui);
                    separated = true;
                }
                let note = if label.contains("Linux") {
                    Some("Linux")
                } else if label.contains("macOS") {
                    Some("macOS")
                } else {
                    None
                };
                if menu_button_item(ui, label, note).clicked() {
                    self.send_combo(keys);
                    ui.close();
                }
            }
            let right_chevron_atom = egui::AtomKind::closure(|_ui, _args| {
                let size = egui::vec2(8.0, 10.0);
                egui::IntoSizedResult {
                    intrinsic_size: size,
                    sized: egui::SizedAtomKind::Paint {
                        paint: std::sync::Arc::new(|ui: &egui::Ui, args: egui::AtomPaintArgs| {
                            theme::chevron(
                                ui.painter(),
                                args.rect.center(),
                                3.5,
                                theme::ChevronDir::Right,
                                theme::MUTED,
                            );
                        }),
                        size,
                    },
                }
            });
            let sub_btn = egui::Button::new(
                egui::RichText::new("Function keys")
                    .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                    .color(theme::TEXT),
            )
            .min_size(egui::vec2(ui.available_width(), 28.0))
            .right_text(right_chevron_atom);

            let sub_menu = egui::containers::menu::SubMenuButton::from_button(sub_btn).config(
                egui::containers::menu::MenuConfig::new().style(|style: &mut egui::Style| {
                    egui::containers::menu::menu_style(style);
                    style.spacing.item_spacing = egui::vec2(0.0, 1.0);
                    style.spacing.interact_size.y = 28.0;
                    style.spacing.button_padding = egui::vec2(10.0, 4.0);
                    style.visuals.widgets.hovered.weak_bg_fill = theme::CARD_HOVER;
                    style.visuals.widgets.hovered.bg_fill = theme::CARD_HOVER;
                    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
                    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
                }),
            );

            sub_menu.ui(ui, |ui| {
                ui.set_width(120.0);
                for key in actions::FUNCTION_KEYS {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(key)
                                    .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                                    .color(theme::TEXT),
                            )
                            .min_size(egui::vec2(ui.available_width(), 28.0)),
                        )
                        .clicked()
                    {
                        self.send_combo(&[key]);
                        ui.close();
                    }
                }
            });
        });
        let hint = if self.inserts_via_agent() {
            "Insert the clipboard in one step through the agent on the target (Right Cmd + V)"
        } else {
            "Type the clipboard key by key: no agent connected on the target (Right Cmd + V)"
        };
        if theme::plain_button(ui, "Paste")
            .on_hover_text(hint)
            .clicked()
        {
            self.request_type_clipboard();
        }
        let agent_up = self.inserts_via_agent();
        let capture_id = ui.make_persistent_id("capture_menu_popup");
        show_dropdown_menu(ui, capture_id, "Capture", 240.0, |ui| {
            if menu_button_item_enabled(
                ui,
                true,
                "Save device snapshot",
                "JPEG from the KVM capture, saved to ~/Downloads",
                "",
            )
            .clicked()
            {
                self.snapshot();
                ui.close();
            }
            if menu_button_item_enabled(
                ui,
                agent_up,
                "Save exact screenshot",
                "Pixel-exact, taken by the agent on the target (all monitors)",
                "Needs the agent connected on the target",
            )
            .clicked()
            {
                self.exact_screenshot();
                ui.close();
            }
            if menu_button_item_enabled(
                ui,
                agent_up,
                "Copy region as image",
                "Drag a rectangle on the video; copied pixel-exact from the agent's screenshot",
                "Needs the agent connected on the target",
            )
            .clicked()
            {
                self.pick_region();
                ui.close();
            }
            if menu_button_item_enabled(
                ui,
                agent_up,
                "Copy screenshot as image",
                "The main monitor, pixel-exact, from the agent",
                "Needs the agent connected on the target",
            )
            .clicked()
            {
                self.copy_screen_image([0.0, 0.0, 1.0, 1.0]);
                ui.close();
            }
            if menu_button_item_enabled(
                ui,
                true,
                "Copy screen text (OCR)",
                "Recognized on this Mac; from the agent's exact screenshot when available",
                "",
            )
            .clicked()
            {
                self.copy_screen_text();
                ui.close();
            }
        });
        if self.caps.supports(Feature::Atx) {
            let power_id = ui.make_persistent_id("power_menu_popup");
            show_dropdown_menu(ui, power_id, "Power", 200.0, |ui| {
                let items = [
                    ("Power on", Confirm::Power(PowerAction::On)),
                    ("Shut down", Confirm::Power(PowerAction::Off)),
                    ("Force off", Confirm::Power(PowerAction::OffHard)),
                    ("Reset", Confirm::Power(PowerAction::ResetHard)),
                    ("Press power button", Confirm::Click(AtxButton::Power)),
                ];
                for (label, c) in items {
                    if menu_button_item(ui, label, None).clicked() {
                        self.confirm = Some(c);
                        ui.close();
                    }
                }
            });
        }

        // Right: the viewer itself.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (icon, tip) = if self.fullscreen {
                (
                    theme::Icon::ExitFullscreen,
                    "Exit fullscreen (Right Cmd + F)",
                )
            } else {
                (theme::Icon::Fullscreen, "Fullscreen (Right Cmd + F)")
            };
            if theme::icon_button(ui, icon, tip).clicked() {
                action = ConsoleAction::ToggleFullscreen;
            }
            let panel_open = self.drawer.is_some();
            if theme::toggle_button(ui, "Panel", panel_open)
                .on_hover_text("Target, video and settings")
                .clicked()
            {
                self.drawer = if panel_open { None } else { Some(Tab::Target) };
            }
            if theme::actions_button(ui)
                .on_hover_text("Search every action (Right Cmd + K)")
                .clicked()
            {
                self.palette = Some(String::new());
            }
        });
        action
    }
}
