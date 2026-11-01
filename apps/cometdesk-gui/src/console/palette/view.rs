//! Modal UI for the action search palette: query input, grouped results and keyboard navigation.
//! Entry point: `Console::palette_ui`.

use super::{Cmd, CmdGroup};
use crate::console::{Console, ConsoleAction};
use crate::theme;

type GroupSlice = (CmdGroup, usize, usize);

impl Console {
    /// The action search: type to filter, Up/Down to move, Enter to run, Esc to close.
    pub(in crate::console) fn palette_ui(&mut self, ctx: &egui::Context) -> ConsoleAction {
        let Some(mut query) = self.palette.take() else {
            return ConsoleAction::None;
        };
        let needle = query.to_lowercase();
        let (mut ordered_matches, group_slices) = self.matched_commands(&needle);

        #[allow(deprecated)]
        let sel_id = egui::Id::new("palette-selected");
        let mut sel = ctx.data(|d| d.get_temp::<usize>(sel_id)).unwrap_or(0);
        let (down, up, close) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.key_pressed(egui::Key::Escape),
            )
        });
        let last = ordered_matches.len().saturating_sub(1);
        if down {
            sel = (sel + 1).min(last);
        }
        if up {
            sel = sel.saturating_sub(1);
        }
        let before = query.clone();
        let mut chosen = None;
        let mut enter_pressed = false;

        egui::Modal::new(egui::Id::unique("actions"))
            .frame(theme::modal_frame().inner_margin(egui::Margin::ZERO))
            .show(ctx, |ui| {
                ui.set_width(480.0);
                Self::palette_header(ui, &mut query, &mut enter_pressed);
                Self::palette_results(ui, &ordered_matches, &group_slices, sel, &mut chosen);
                Self::palette_footer(ui);
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    enter_pressed = true;
                }
            });

        if enter_pressed && chosen.is_none() && !ordered_matches.is_empty() {
            chosen = Some(sel.min(last));
        }

        if query != before {
            sel = 0;
        }
        ctx.data_mut(|d| d.insert_temp(sel_id, sel));
        if let Some(i) = chosen {
            ctx.data_mut(|d| d.insert_temp(sel_id, 0usize));
            let (_, cmd) = ordered_matches.swap_remove(i);
            return self.run_command(cmd);
        }
        if !close {
            self.palette = Some(query);
        } else {
            ctx.data_mut(|d| d.insert_temp(sel_id, 0usize));
        }
        ConsoleAction::None
    }

    fn matched_commands(&self, needle: &str) -> (Vec<(String, Cmd)>, Vec<GroupSlice>) {
        let matches: Vec<(String, Cmd)> = self
            .commands()
            .into_iter()
            .filter(|(l, _)| l.to_lowercase().contains(needle))
            .collect();

        let mut ordered_matches: Vec<(String, Cmd)> = Vec::new();
        let mut group_slices: Vec<(CmdGroup, usize, usize)> = Vec::new();
        for g in CmdGroup::ALL {
            let start = ordered_matches.len();
            for (label, cmd) in &matches {
                if cmd.group() == g {
                    ordered_matches.push((label.clone(), *cmd));
                }
            }
            if ordered_matches.len() > start {
                group_slices.push((g, start, ordered_matches.len()));
            }
        }
        (ordered_matches, group_slices)
    }

    fn palette_header(ui: &mut egui::Ui, query: &mut String, enter_pressed: &mut bool) {
        // Header row: 52px high, search icon, borderless 15px text input, right kbd("esc")
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);
                    let (icon_rect, _) =
                        ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                    theme::draw_icon(ui.painter(), icon_rect, theme::Icon::Search, theme::MUTED);

                    let edit = ui.add(
                        egui::TextEdit::singleline(query)
                            .desired_width(ui.available_width() - 48.0)
                            .font(egui::FontId::new(15.0, egui::FontFamily::Proportional))
                            .hint_text("Search actions…")
                            .frame(egui::Frame::NONE),
                    );
                    edit.request_focus();

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        theme::kbd(ui, "esc");
                    });

                    if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        *enter_pressed = true;
                    }
                });
            });

        // 1px rule under header
        let (rule_rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rule_rect, egui::CornerRadius::ZERO, theme::BORDER);
    }

    fn palette_results(
        ui: &mut egui::Ui,
        ordered_matches: &[(String, Cmd)],
        group_slices: &[GroupSlice],
        sel: usize,
        chosen: &mut Option<usize>,
    ) {
        egui::Frame::NONE
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                if ordered_matches.is_empty() {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("No action matches.").color(theme::MUTED));
                    ui.add_space(8.0);
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(340.0)
                        .show(ui, |ui| {
                            for (group, start_idx, end_idx) in group_slices {
                                ui.add_space(6.0);
                                ui.label(
                                    egui::RichText::new(group.label().to_uppercase())
                                        .font(egui::FontId::new(11.0, theme::bold_family()))
                                        .color(theme::MUTED),
                                );
                                ui.add_space(2.0);

                                for (offset, (label, cmd)) in
                                    ordered_matches[*start_idx..*end_idx].iter().enumerate()
                                {
                                    let i = *start_idx + offset;
                                    Self::palette_row(ui, i, label, *cmd, sel, chosen);
                                }
                            }
                        });
                }
            });
    }

    fn palette_row(
        ui: &mut egui::Ui,
        i: usize,
        label: &str,
        cmd: Cmd,
        sel: usize,
        chosen: &mut Option<usize>,
    ) {
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), egui::Sense::click());
        let on = i == sel;
        if on || resp.hovered() {
            let fill = if on { theme::CARD_HOVER } else { theme::CARD };
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(6), fill);
        }
        if on {
            resp.scroll_to_me(None);
        }

        // Left label (13.5px)
        let label_font = egui::FontId::new(
            13.5,
            if on {
                theme::bold_family()
            } else {
                egui::FontFamily::Proportional
            },
        );
        let label_galley = ui
            .painter()
            .layout_no_wrap(label.to_owned(), label_font, theme::TEXT);
        let label_pos = egui::pos2(
            rect.min.x + 12.0,
            rect.center().y - label_galley.size().y * 0.5,
        );
        ui.painter().galley(label_pos, label_galley, theme::TEXT);

        // Right hint
        if on {
            let hint_font = egui::FontId::new(12.0, theme::bold_family());
            let hint_galley =
                ui.painter()
                    .layout_no_wrap("Enter run".to_owned(), hint_font, theme::ACCENT);
            let hint_pos = egui::pos2(
                rect.max.x - 12.0 - hint_galley.size().x,
                rect.center().y - hint_galley.size().y * 0.5,
            );
            ui.painter().galley(hint_pos, hint_galley, theme::ACCENT);
        } else if cmd.is_confirm_gated() {
            let hint_font = egui::FontId::new(12.0, egui::FontFamily::Proportional);
            let hint_galley =
                ui.painter()
                    .layout_no_wrap("asks first".to_owned(), hint_font, theme::MUTED);
            let hint_pos = egui::pos2(
                rect.max.x - 12.0 - hint_galley.size().x,
                rect.center().y - hint_galley.size().y * 0.5,
            );
            ui.painter().galley(hint_pos, hint_galley, theme::MUTED);
        } else if let Some(shortcut) = cmd.shortcut_text() {
            let hint_font = egui::FontId::new(11.5, egui::FontFamily::Monospace);
            let hint_galley =
                ui.painter()
                    .layout_no_wrap(shortcut.to_owned(), hint_font, theme::MUTED);
            let hint_pos = egui::pos2(
                rect.max.x - 12.0 - hint_galley.size().x,
                rect.center().y - hint_galley.size().y * 0.5,
            );
            ui.painter().galley(hint_pos, hint_galley, theme::MUTED);
        }

        if resp.clicked() {
            *chosen = Some(i);
        }
    }

    fn palette_footer(ui: &mut egui::Ui) {
        // 1px rule above footer
        let (footer_rule, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(footer_rule, egui::CornerRadius::ZERO, theme::BORDER);

        // Footer row: "Up/Down move · Enter run · Esc close"
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(16, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(16.0, 0.0);
                    ui.label(
                        egui::RichText::new("Up/Down move · Enter run · Esc close")
                            .font(egui::FontId::new(11.5, egui::FontFamily::Proportional))
                            .color(theme::MUTED),
                    );
                });
            });
    }
}
