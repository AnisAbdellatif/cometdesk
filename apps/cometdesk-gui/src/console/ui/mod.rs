//! Per-frame egui rendering entry point for the connected console.
//! Entry points: `Console::ui`, `Console::toolbar_visible`.

mod dialogs;
mod drawer;
mod overlays;
mod panels;
mod status;
mod toolbar;

use std::time::Instant;

use cometdesk::api::capabilities::Feature;
use cometdesk_media::Transport;

use super::clip::health_text;
use super::{Console, ConsoleAction, EDGE_DWELL};
use crate::clipsync::{ClipBridge, Note};
use crate::theme;

impl Console {
    /// Whether the toolbar is drawn this frame (always, except in fullscreen).
    pub fn toolbar_visible(&self, edge_dwelled: bool) -> bool {
        !self.fullscreen
            || self.toolbar_hover
            || edge_dwelled
            || self.confirm.is_some()
            || self.drawer.is_some()
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> ConsoleAction {
        let mut action = ConsoleAction::None;
        self.media.poll();
        let dropped: Vec<_> = ui.ctx().input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_owned())
                .collect()
        });
        let mut dropped = dropped;
        dropped.append(&mut self.picked_files.lock().unwrap());
        if !dropped.is_empty() {
            if self.inserts_via_agent() {
                self.send_files(dropped);
            } else if self.caps.supports(Feature::Msd) {
                self.media.dropped(dropped);
            }
        }
        if let Some(notes) = self.clip.as_ref().map(ClipBridge::take_notes) {
            for n in notes {
                match n {
                    Note::Info(text) => self.notify(text),
                    Note::Target { title, body } => self.target_note(&title, &body),
                }
            }
        }
        self.poll_transfers(ui.ctx());
        if self.browser.is_open() {
            match self.clip.as_ref().filter(|_| self.inserts_via_agent()) {
                Some(clip) => self.browser.ui(ui.ctx(), clip, &self.rt, &self.proxy),
                None => self.browser.close(),
            }
        }
        if let Some(rx) = self.inserting.as_mut() {
            match rx.try_recv() {
                Ok(Ok(msg)) => {
                    self.inserting = None;
                    self.notify(msg);
                }
                Ok(Err(e)) => {
                    self.inserting = None;
                    self.notify(format!("Insert failed: {e}"));
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    self.inserting = None;
                    self.notify("Insert failed: connection to the agent lost".into());
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = self.video_info_rx.as_mut() {
            match rx.try_recv() {
                Ok(info) => {
                    let new_transport = super::video::transport_for_mode(info.mode);
                    let transport_changed = new_transport != self.transport;
                    // WebRTC sources get the angle in their handshake: restart to apply it.
                    let rotated = info.orientation != self.video_info.orientation
                        && new_transport != Transport::Direct;
                    let codec_changed = info.codec != self.video_info.codec;
                    let mode_changed = info.mode != self.video_info.mode;
                    // Venc and latency changes restart the device-side encoder; the existing
                    // connection goes stale unless the client reconnects.
                    let encoder_restarted = info.venc != self.video_info.venc
                        || info.latency != self.video_info.latency;
                    self.transport = new_transport;
                    self.video_info = info;
                    self.video_info_rx = None;
                    if transport_changed || rotated || codec_changed || encoder_restarted {
                        self.stream = None;
                        self.start_stream();
                    } else if mode_changed && self.audio_on {
                        self.set_audio(true);
                    }
                    // The mode picks the audio source, so a saved "on" waits for it.
                    if self.audio_on && self.stream.as_ref().is_some_and(|s| s.audio.is_none()) {
                        self.set_audio(true);
                    }
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => self.video_info_rx = None,
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
            }
        }
        match self.setup.take_outcome() {
            Some(Ok(msg)) => self.notify(msg),
            Some(Err(e)) => self.notify(format!("Bridge setup failed: {e}")),
            None => {}
        }
        // Paired before, but the Comet side is gone or broken (e.g. after a firmware upgrade).
        if !self.repair_noticed
            && self.clip.is_some()
            && let Some(h) = self.setup.health()
        {
            self.repair_noticed = true;
            if h.needs_setup() {
                self.notify(format!(
                    "Clipboard: {}; repair it from the Clipboard menu",
                    health_text(&h)
                ));
            }
        }
        let pointer = ui.ctx().pointer_latest_pos();
        // The popups of a menu are outside the toolbar: while one is open the pointer is
        // not over the toolbar, which must not hide it (and the menu with it).
        self.menu_open = egui::Popup::is_any_open(ui.ctx());
        let at_edge = self.fullscreen && pointer.is_some_and(|p| p.y <= 6.0);
        if !at_edge {
            self.edge_since = None;
        }
        let edge_dwelled = at_edge && {
            let since = *self.edge_since.get_or_insert_with(Instant::now);
            let rested = since.elapsed();
            if rested < EDGE_DWELL {
                ui.ctx().request_repaint_after(EDGE_DWELL - rested);
            }
            rested >= EDGE_DWELL
        };
        if self.menu_open || self.toolbar_visible(edge_dwelled) {
            let frame = egui::Frame::NONE
                .fill(theme::PANEL)
                .stroke(egui::Stroke::NONE)
                .inner_margin(egui::Margin::symmetric(12, 8));
            let resp = egui::Panel::top("toolbar").frame(frame).show(ui, |ui| {
                // Flat buttons: only hover and open states get a background.
                let w = &mut ui.visuals_mut().widgets;
                for s in [&mut w.inactive, &mut w.noninteractive] {
                    s.weak_bg_fill = egui::Color32::TRANSPARENT;
                    s.bg_stroke = egui::Stroke::NONE;
                }
                ui.horizontal_centered(|ui| action = self.toolbar(ui));
            });
            // The signal edge: the toolbar's lower rule carries the connection state.
            let rect = resp.response.rect;
            let (_, signal) = self.status();
            let signal = if signal == theme::OK {
                signal.gamma_multiply(0.4)
            } else {
                signal
            };
            ui.painter().hline(
                rect.x_range(),
                rect.bottom() - 0.5,
                egui::Stroke::new(1.0, signal),
            );
            self.toolbar_hover = self.fullscreen && resp.response.contains_pointer();
            self.toolbar_rect = Some(resp.response.rect);
            self.menu_open = egui::Popup::is_any_open(ui.ctx());
        } else {
            self.toolbar_hover = false;
            self.toolbar_rect = None;
        }

        let central = egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                self.overlays(ui);
            });
        self.video_area = central.response.rect;
        if self.drawer_ui(ui.ctx()) {
            self.target_changed = true;
        }
        let from_palette = self.palette_ui(ui.ctx());
        if from_palette != ConsoleAction::None {
            action = from_palette;
        }
        self.dialogs(ui.ctx());
        self.adopt_agent_os();
        // Only clear the flag when it is actually propagated: another action this frame must
        // not drop a pending save.
        if self.target_changed && action == ConsoleAction::None {
            self.target_changed = false;
            action = ConsoleAction::TargetChanged;
        }
        action
    }
}
