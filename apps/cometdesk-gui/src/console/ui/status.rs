//! Connection, stream, signal and HID status line computation.
//! Entry point: `Console::status`.

use cometdesk::events::ConnState;

use crate::console::Console;
use crate::theme;

impl Console {
    pub(in crate::console) fn status(&self) -> (String, egui::Color32) {
        let red = theme::ERR;
        let amber = theme::WARN;
        match &*self.conn.borrow() {
            ConnState::Connected => {}
            ConnState::Kicked => return ("Disconnected by the device".into(), red),
            ConnState::Disconnected(_) | ConnState::Connecting => {
                return ("Reconnecting…".into(), amber);
            }
            ConnState::Closed => return ("Closed".into(), red),
        }
        if self.stream.is_none() {
            return ("Paused (window hidden)".into(), amber);
        }
        let state = self.state.borrow();
        if state.video_signal() == Some(false) {
            return ("No signal".into(), amber);
        }
        if state.hid().is_some_and(|h| !h.online) {
            return ("Keyboard/mouse offline".into(), amber);
        }
        ("Connected".into(), theme::OK)
    }
}
