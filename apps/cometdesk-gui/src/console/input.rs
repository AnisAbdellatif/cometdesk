//! Keyboard and mouse input forwarding, shortcuts, modal hit-testing and automation hooks.
//! Entry points: `Console::on_key`, `Console::on_cursor`, `Console::on_button`, `Console::send_hid`.

use std::sync::atomic::Ordering;

use cometdesk::api::streamer::SnapshotOptions;
use cometdesk::hid::{HidCommand, MouseButton};
use winit::keyboard::KeyCode;

use super::Console;
use crate::actions::{self, HostModifiers, Shortcut};

impl Console {
    // ----- automation (see app/control.rs) -----------------------------------------------

    pub fn send_hid(&self, cmds: impl IntoIterator<Item = HidCommand>) {
        for cmd in cmds {
            self.hid.send(cmd);
        }
    }

    /// Types `text` on the target without the confirmation a person gets.
    pub fn control_type(&mut self, text: String) {
        self.release_all();
        self.deliver_text(text);
    }

    pub fn control_status(&self) -> serde_json::Value {
        let state = self.state.borrow();
        let (status, _) = self.status();
        serde_json::json!({
            "device": self.device.display_name(),
            "status": status,
            "connection": format!("{:?}", &*self.conn.borrow()),
            "video_signal": state.video_signal(),
            "transport": format!("{:?}", self.transport),
            "fullscreen": self.fullscreen,
            "stats": self.stats.shown,
            "notice": self.notice.as_ref().map(|n| &n.0),
            "typing": self.typing.as_ref().map(|t| serde_json::json!({
                "done": t.done.load(Ordering::Relaxed), "total": t.total,
            })),
            "modal": self.has_modal(),
            "agent": self.agent().is_some(),
        })
    }

    /// Saves the device's snapshot of the target; with `ocr`, also reads the text on it.
    pub fn control_snapshot(
        &self,
        path: std::path::PathBuf,
        ocr: bool,
        reply: std::sync::mpsc::Sender<serde_json::Value>,
    ) {
        let session = self.session.clone();
        self.rt.spawn(async move {
            let r = async {
                let jpeg = session
                    .snapshot(SnapshotOptions::default())
                    .await
                    .map_err(|e| format!("snapshot failed: {e}"))?;
                tokio::fs::write(&path, &jpeg)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut v = serde_json::json!({"ok": true, "path": path, "bytes": jpeg.len()});
                if ocr {
                    let img = jpeg.to_vec();
                    v["text"] = tokio::task::spawn_blocking(move || crate::ocr::recognize(&img))
                        .await
                        .map_err(|e| e.to_string())??
                        .into();
                }
                Ok::<_, String>(v)
            }
            .await;
            let _ = reply.send(r.unwrap_or_else(|e| serde_json::json!({"ok": false, "error": e})));
        });
    }

    // ----- input -----------------------------------------------------------------------

    fn flush(&mut self) {
        for cmd in self.cmds.drain(..) {
            self.hid.send(cmd);
        }
    }

    /// A key event while input is captured. Returns an app shortcut instead of forwarding it.
    pub fn on_key(&mut self, code: KeyCode, pressed: bool, repeat: bool) -> Option<Shortcut> {
        self.mods.update(code, pressed);
        if actions::is_host_key(code) {
            return None; // reserved, never sent
        }
        if pressed
            && !repeat
            && let Some(s) = actions::shortcut(code, self.mods, self.device.target)
        {
            return Some(s);
        }
        self.mapper.key(code, pressed, repeat, &mut self.cmds);
        self.flush();
        None
    }

    pub fn on_cursor(&mut self, pos: (f64, f64)) {
        self.mapper.cursor(pos, &mut self.cmds);
        self.flush();
    }

    pub fn on_cursor_left(&mut self) {
        self.mapper.cursor_left();
    }

    pub fn cursor_inside(&self) -> bool {
        self.mapper.cursor_inside()
    }

    pub fn on_button(&mut self, button: MouseButton, pressed: bool) {
        self.mapper.button(button, pressed, &mut self.cmds);
        self.flush();
    }

    pub fn on_wheel_lines(&mut self, dx: f64, dy: f64) {
        self.mapper.wheel_lines(dx, dy, &mut self.cmds);
        self.flush();
    }

    pub fn on_wheel_pixels(&mut self, dx: f64, dy: f64) {
        self.mapper.wheel_pixels(dx, dy, &mut self.cmds);
        self.flush();
    }

    /// Focus lost or capture paused: nothing may stay pressed on the target.
    pub fn release_all(&mut self) {
        self.mods = HostModifiers::default();
        self.mapper.focus_lost(&mut self.cmds);
        self.flush();
    }

    /// Whether `pos` (points) is on the toolbar.
    pub fn over_toolbar(&self, pos: egui::Pos2) -> bool {
        self.toolbar_rect.is_some_and(|r| r.contains(pos))
    }

    /// Opens the action search (Right Cmd + K).
    pub fn open_palette(&mut self) {
        self.palette = Some(String::new());
    }

    pub fn menu_open(&self) -> bool {
        self.menu_open
    }

    pub fn has_modal(&self) -> bool {
        self.confirm.is_some()
            || self.target_setup.is_some()
            || self.region.is_some()
            || self.palette.is_some()
    }

    pub fn send_combo(&mut self, keys: &[&'static str]) {
        for cmd in actions::combo(keys) {
            self.hid.send(cmd);
        }
    }

    pub fn set_window_focused(&mut self, focused: bool) {
        self.window_focused = focused;
    }
}
