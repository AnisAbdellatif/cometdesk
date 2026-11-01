//! Right-hand side drawer container and Target / Video / Settings tab switcher.
//! Entry point: `Console::drawer_ui`.

use crate::console::Console;
use crate::console::state::Tab;
use crate::theme;

impl Console {
    /// The right-hand panel: everything that is not one of the four everyday actions.
    /// True when the saved device changed.
    pub(super) fn drawer_ui(&mut self, ctx: &egui::Context) -> bool {
        let Some(tab) = self.drawer else { return false };
        let top = self.toolbar_rect.map_or(0.0, |r| r.bottom());
        let height = (ctx.content_rect().height() - top).max(240.0);
        let mut changed = false;
        let mut next = tab;
        #[allow(deprecated)] // a stable id keeps the panel's scroll position
        let panel_id = egui::Id::new("panel");
        egui::Area::new(panel_id)
            .anchor(egui::Align2::RIGHT_TOP, [0.0, top])
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::ZERO)
                    .show(ui, |ui| {
                        ui.set_width(320.0);
                        ui.set_height(height);
                        ui.set_max_width(320.0);
                        ui.set_max_height(height);

                        // 1px left rule
                        let panel_rect = ui.max_rect();
                        ui.painter().vline(
                            panel_rect.left() + 0.5,
                            panel_rect.y_range(),
                            egui::Stroke::new(1.0, theme::BORDER),
                        );

                        // Tab header bar
                        let tab_bar_height = 42.0;
                        let (tab_bar_rect, _) = ui.allocate_exact_size(
                            egui::vec2(320.0, tab_bar_height),
                            egui::Sense::hover(),
                        );
                        // Bottom rule for tab header
                        ui.painter().hline(
                            tab_bar_rect.x_range(),
                            tab_bar_rect.bottom() - 0.5,
                            egui::Stroke::new(1.0, theme::BORDER),
                        );

                        let mut cur_x = tab_bar_rect.min.x + 12.0;
                        for t in Tab::ALL {
                            let active = tab == t;
                            let font_id = if active {
                                egui::FontId::new(14.0, theme::bold_family())
                            } else {
                                egui::FontId::new(14.0, egui::FontFamily::Proportional)
                            };
                            let text_color = if active { theme::TEXT } else { theme::MUTED };
                            let galley = ui.painter().layout_no_wrap(
                                t.label().to_owned(),
                                font_id,
                                text_color,
                            );
                            let tab_w = galley.size().x + 24.0;
                            let tab_rect = egui::Rect::from_min_size(
                                egui::pos2(cur_x, tab_bar_rect.min.y),
                                egui::vec2(tab_w, tab_bar_height),
                            );
                            let resp = ui.interact(
                                tab_rect,
                                ui.scope_id().with(t.label()),
                                egui::Sense::click(),
                            );
                            if resp.clicked() {
                                next = t;
                            }
                            let label_color = if active || resp.hovered() {
                                theme::TEXT
                            } else {
                                theme::MUTED
                            };
                            let text_pos = egui::pos2(cur_x + 12.0, tab_rect.min.y + 14.0);
                            ui.painter().galley(text_pos, galley, label_color);
                            if active {
                                ui.painter().hline(
                                    tab_rect.x_range(),
                                    tab_bar_rect.bottom() - 1.0,
                                    egui::Stroke::new(2.0, theme::ACCENT),
                                );
                            }
                            cur_x += tab_w;
                        }

                        theme::set_inline_sections(ui, true);
                        egui::ScrollArea::vertical()
                            .max_height(height - tab_bar_height)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                egui::Frame::NONE.inner_margin(egui::Margin::same(16)).show(
                                    ui,
                                    |ui| {
                                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 12.0);
                                        match tab {
                                            Tab::Target => changed = self.target_panel(ui),
                                            Tab::Video => self.video_menu(ui),
                                            Tab::Settings => changed = self.settings_panel(ui),
                                        }
                                    },
                                );
                            });
                    });
            });
        self.drawer = Some(next);
        changed
    }
}
