//! System info and virtual media collapsible sections in the Target drawer tab.
//! Entry point: `Console::info_menu`.

use cometdesk::api::capabilities::Feature;
use cometdesk_clip::{Hello, Info, SessionState};

use crate::console::{Console, debug_view};
use crate::theme;

impl Console {
    /// What the agent reports, as a menu: for checking at a glance that the target is what
    /// the app assumes.
    pub(in crate::console) fn info_menu(&mut self, ui: &mut egui::Ui) {
        let has_media =
            self.caps.supports(Feature::Msd) || debug_view().as_deref() == Some("panel-target");

        let card_frame = egui::Frame::NONE
            .fill(theme::CARD)
            .stroke(egui::Stroke::new(1.0, theme::BORDER))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::ZERO);

        card_frame.show(ui, |ui| {
            self.system_info_section(ui);
            if has_media {
                self.media_section(ui);
            }
        });
    }

    fn system_info_section(&mut self, ui: &mut egui::Ui) {
        let (info_head_rect, info_resp) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::click());
        if ui.is_rect_visible(info_head_rect) {
            if info_resp.hovered() {
                ui.painter().rect_filled(
                    info_head_rect,
                    egui::CornerRadius::same(10),
                    theme::CARD_HOVER,
                );
            }
            let title_font = egui::FontId::new(14.0, theme::bold_family());
            let title_galley =
                ui.painter()
                    .layout_no_wrap("System info".to_owned(), title_font, theme::TEXT);
            let title_pos = egui::pos2(
                info_head_rect.min.x + 14.0,
                info_head_rect.center().y - title_galley.size().y * 0.5,
            );
            ui.painter().galley(title_pos, title_galley, theme::TEXT);

            let chevron_dir = if self.info_expanded {
                theme::ChevronDir::Down
            } else {
                theme::ChevronDir::Right
            };
            let chevron_pos =
                egui::pos2(info_head_rect.max.x - 14.0 - 4.0, info_head_rect.center().y);
            theme::chevron(ui.painter(), chevron_pos, 4.0, chevron_dir, theme::MUTED);
        }
        if info_resp.clicked() {
            self.info_expanded = !self.info_expanded;
        }

        if self.info_expanded {
            egui::Frame::NONE
                .inner_margin(egui::Margin {
                    left: 14,
                    right: 14,
                    top: 0,
                    bottom: 14,
                })
                .show(ui, |ui| {
                    if let Some((hello, info)) = self.agent() {
                        self.agent_info_body(ui, hello, info);
                    } else {
                        ui.label(
                            egui::RichText::new("No target agent connected")
                                .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                                .color(theme::MUTED),
                        );
                    }
                });
        }
    }

    fn agent_info_body(&mut self, ui: &mut egui::Ui, hello: Hello, info: Info) {
        let up = match (info.uptime_secs, self.uptime_seen) {
            (Some(u), Some((seen, at))) if seen == u => Some(u + at.elapsed().as_secs()),
            (u, _) => u,
        };
        let layouts: Vec<&str> = info
            .layouts
            .iter()
            .filter_map(|l| cometdesk::layout::keymap_from_native(l))
            .collect();
        let mut rows: Vec<(&str, String)> = vec![
            ("Host", hello.name.clone()),
            ("OS", info.os_version.clone().unwrap_or(hello.os.clone())),
            ("User", info.user.clone().unwrap_or_default()),
            (
                "Session",
                match info.session {
                    SessionState::Locked => "locked (lock screen)".into(),
                    SessionState::Active => "active".into(),
                    SessionState::Unknown => "unknown".into(),
                },
            ),
            ("Keyboard", info.keyboard.clone().unwrap_or_default()),
            ("Layouts", layouts.join(", ")),
        ];
        rows.push((
            "Input method",
            match info.ime {
                Some(true) => "on (typing key by key may be composed)".into(),
                Some(false) => "off".into(),
                None => String::new(),
            },
        ));
        if let Some(f) = &info.focus {
            rows.push((
                "Focus",
                format!("{}{}", f.app, if f.terminal { " (terminal)" } else { "" }),
            ));
        }
        for (i, d) in info.displays.iter().enumerate() {
            rows.push((
                "Display",
                format!(
                    "{}: {}x{} at {},{} · {}%{}",
                    i + 1,
                    d.width,
                    d.height,
                    d.x,
                    d.y,
                    d.scale,
                    if d.primary { " · main" } else { "" }
                ),
            ));
        }
        if let Some(u) = up {
            rows.push(("Uptime", format!("{}h {:02}m", u / 3600, u / 60 % 60)));
        }
        if let Some(b) = info.battery {
            rows.push((
                "Battery",
                format!(
                    "{}%{}",
                    b.percent,
                    if b.charging { " (charging)" } else { "" }
                ),
            ));
        }
        if let Some(l) = info.load {
            let usage = |u: cometdesk_clip::Usage| {
                format!(
                    "{:.1} / {:.1} GB ({}%)",
                    u.used_mb as f64 / 1024.0,
                    u.total_mb as f64 / 1024.0,
                    (u.used_mb * 100).checked_div(u.total_mb).unwrap_or(0)
                )
            };
            if let Some(c) = l.cpu_percent {
                rows.push(("CPU", format!("{c}%")));
            }
            if let Some(m) = l.memory {
                rows.push(("Memory", usage(m)));
            }
            if let Some(d) = l.disk {
                rows.push(("Disk", usage(d)));
            }
        }
        egui::Grid::new("agent_info")
            .num_columns(2)
            .spacing(egui::vec2(12.0, 4.0))
            .min_col_width(64.0)
            .show(ui, |ui| {
                for (k, v) in rows.iter().filter(|(_, v)| !v.is_empty()) {
                    ui.label(
                        egui::RichText::new(*k)
                            .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                            .color(theme::MUTED),
                    );
                    ui.label(
                        egui::RichText::new(v)
                            .font(egui::FontId::new(13.0, egui::FontFamily::Proportional))
                            .color(theme::TEXT),
                    );
                    ui.end_row();
                }
            });
        if let Some(v) = info.volume {
            ui.separator();
            self.volume_row(ui, v);
        }
        for (i, d) in info.displays.iter().enumerate() {
            self.display_menu(ui, i, d, info.displays.len());
        }
        if info.layouts.len() > 1 {
            ui.separator();
            theme::section(ui, "Switch the target's layout", |ui| {
                for l in &info.layouts {
                    let label = cometdesk::layout::keymap_from_native(l).unwrap_or(l);
                    let active = info.keyboard.as_deref() == Some(l.as_str());
                    if ui.selectable_label(active, label).clicked() && !active {
                        self.request_layout(l.clone());
                        ui.close();
                    }
                }
            });
        }
    }

    fn media_section(&mut self, ui: &mut egui::Ui) {
        let (sep_rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(sep_rect, egui::CornerRadius::ZERO, theme::BORDER);

        let (media_head_rect, media_resp) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::click());
        if ui.is_rect_visible(media_head_rect) {
            if media_resp.hovered() {
                ui.painter().rect_filled(
                    media_head_rect,
                    egui::CornerRadius::same(10),
                    theme::CARD_HOVER,
                );
            }
            let title_font = egui::FontId::new(14.0, theme::bold_family());
            let title_galley =
                ui.painter()
                    .layout_no_wrap("Media".to_owned(), title_font, theme::TEXT);
            let title_pos = egui::pos2(
                media_head_rect.min.x + 14.0,
                media_head_rect.center().y - title_galley.size().y * 0.5,
            );
            ui.painter()
                .galley(title_pos, title_galley.clone(), theme::TEXT);

            let mounted =
                self.media.is_mounted() || debug_view().as_deref() == Some("panel-target");
            if mounted {
                let status_font = egui::FontId::new(12.0, egui::FontFamily::Proportional);
                let status_galley =
                    ui.painter()
                        .layout_no_wrap("● mounted".to_owned(), status_font, theme::OK);
                let status_pos = egui::pos2(
                    title_pos.x + title_galley.size().x + 8.0,
                    media_head_rect.center().y - status_galley.size().y * 0.5,
                );
                ui.painter().galley(status_pos, status_galley, theme::OK);
            }

            let chevron_dir = if self.media_expanded {
                theme::ChevronDir::Down
            } else {
                theme::ChevronDir::Right
            };
            let chevron_pos = egui::pos2(
                media_head_rect.max.x - 14.0 - 4.0,
                media_head_rect.center().y,
            );
            theme::chevron(ui.painter(), chevron_pos, 4.0, chevron_dir, theme::MUTED);
        }
        if media_resp.clicked() {
            self.media_expanded = !self.media_expanded;
        }

        if self.media_expanded {
            egui::Frame::NONE
                .inner_margin(egui::Margin {
                    left: 14,
                    right: 14,
                    top: 0,
                    bottom: 14,
                })
                .show(ui, |ui| {
                    self.media.panel_ui(ui);
                });
        }
    }
}
