//! Window, event loop and screen switching.
//!
//! Input routing: while a console is focused and no dialog or text field wants the keyboard,
//! key events go to the target, not to egui. Pointer events over the video go to the target;
//! over the toolbar or dialogs they go to egui. Releases always reach the target so nothing
//! stays pressed.

use std::sync::Arc;
use std::time::{Duration, Instant};

use cometdesk::tls::PinStore;
use tokio::runtime::Runtime;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

use crate::console::{Connected, Console, ConsoleAction};
use crate::devices::{ConnectError, ConnectRequest, DevicesUi};
use crate::gpu::Gpu;
use crate::store::{SavedDevice, Store};

mod connect;
pub mod control;
mod events;
#[cfg(target_os = "macos")]
mod macos;
mod window;

pub enum UserEvent {
    Control(control::Request),
    /// A decoded video frame is waiting.
    Frame,
    /// Something shown in the UI changed.
    Refresh,
    Connected(Box<Result<Connected, (ConnectRequest, ConnectError)>>),
    Notice(String),
    /// A keychain password lookup for a device the user clicked Connect on finished.
    Password(SavedDevice, Option<String>),
    /// The stored clipboard pairing secret loaded for the current console.
    ClipSecret(cometdesk_clip::Secret),
}

pub struct App {
    rt: Runtime,
    proxy: EventLoopProxy<UserEvent>,
    pins: Arc<PinStore>,
    iso_fix: bool,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    egui_ctx: egui::Context,
    egui_state: Option<egui_winit::State>,
    devices: DevicesUi,
    console: Option<Console>,
    connecting: Option<String>,
    focused: bool,
    /// egui must run before the next draw (input happened or it asked to repaint).
    egui_dirty: bool,
    /// When egui asked to be repainted next.
    egui_repaint_at: Option<Instant>,
    /// Latest cursor position in physical pixels (egui's own copy can be stale: hovering
    /// the video is not forwarded to egui).
    cursor_px: Option<(f64, f64)>,
    /// The last cursor move was given to egui (so it can clear hover states on leaving).
    egui_had_pointer: bool,
}

impl App {
    pub fn new(
        rt: Runtime,
        proxy: EventLoopProxy<UserEvent>,
        pins: Arc<PinStore>,
        iso_fix: bool,
    ) -> Self {
        let devices = DevicesUi::new(Store::load_default(), rt.handle().clone(), proxy.clone());
        Self {
            rt,
            proxy,
            pins,
            iso_fix,
            window: None,
            gpu: None,
            egui_ctx: egui::Context::default(),
            egui_state: None,
            devices,
            console: None,
            connecting: None,
            focused: true,
            egui_dirty: true,
            egui_repaint_at: None,
            cursor_px: None,
            egui_had_pointer: false,
        }
    }

    /// Disconnect and log out. Call once the event loop has returned.
    pub fn shutdown(&mut self) {
        if let Some(c) = self.console.take() {
            let logout = c.disconnect();
            // Wait for the logout, but no longer than this; the runtime is dropped with the app.
            // The `async` block matters: `timeout` needs the runtime context to build its timer,
            // which only exists once `block_on` is running.
            let _ = self
                .rt
                .block_on(async { tokio::time::timeout(Duration::from_millis(500), logout).await });
        }
    }
}
