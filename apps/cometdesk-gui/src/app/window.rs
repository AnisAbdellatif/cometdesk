//! Window state, input-routing predicates, shortcuts, fullscreen toggle, and redraw loop.
//!
//! Entry points: `App::redraw`, `App::request_redraw`, `App::handle_action`, `App::shortcut`.

use std::time::{Duration, Instant};

use winit::window::Fullscreen;

use super::App;
use crate::actions::Shortcut;
use crate::console::ConsoleAction;

impl App {
    fn pixels_per_point(&self) -> f32 {
        self.egui_ctx.pixels_per_point()
    }

    /// Keyboard goes to the target (not egui).
    pub(super) fn capturing_keyboard(&self) -> bool {
        self.focused
            && self.console.as_ref().is_some_and(|c| !c.has_modal())
            && !self.egui_ctx.egui_wants_keyboard_input()
    }

    /// The pointer is over the video, not over toolbar/dialogs.
    pub(super) fn pointer_to_target(&self) -> bool {
        self.console
            .as_ref()
            .is_some_and(|c| !c.has_modal() && !c.menu_open() && c.cursor_inside())
            && !self.pointer_over_ui()
    }

    /// The cursor is on egui UI: the toolbar, or a menu/popup/overlay layer. Decided from our
    /// own cursor position and last frame's layers. Not `Context::is_pointer_over_egui`: it
    /// counts the central panel behind the video as UI and uses egui's possibly stale
    /// pointer position, which made clicks on the video randomly not reach the target.
    fn pointer_over_ui(&self) -> bool {
        let Some((x, y)) = self.cursor_px else {
            return false;
        };
        let ppp = f64::from(self.pixels_per_point());
        let pos = egui::pos2((x / ppp) as f32, (y / ppp) as f32);
        if self.console.as_ref().is_some_and(|c| c.over_toolbar(pos)) {
            return true;
        }
        self.egui_ctx
            .layer_id_at(pos)
            .is_some_and(|layer| layer.order != egui::Order::Background)
    }

    pub(super) fn set_title(&self) {
        if let Some(w) = &self.window {
            match &self.console {
                Some(c) => w.set_title(&format!("cometdesk — {}", c.device.display_name())),
                None => w.set_title("CometDesk"),
            }
        }
    }

    fn toggle_fullscreen(&mut self) {
        let (Some(window), Some(console)) = (&self.window, &mut self.console) else {
            return;
        };
        console.fullscreen = !console.fullscreen;
        window.set_fullscreen(console.fullscreen.then_some(Fullscreen::Borderless(None)));
        self.egui_dirty = true;
    }

    pub(super) fn shortcut(&mut self, s: Shortcut) {
        match s {
            Shortcut::ToggleFullscreen => self.toggle_fullscreen(),
            Shortcut::TypeClipboard => {
                if let Some(c) = &mut self.console {
                    c.request_type_clipboard();
                }
            }
            Shortcut::ToggleStats => {
                if let Some(c) = &mut self.console {
                    c.show_stats = !c.show_stats;
                }
            }
            Shortcut::Palette => {
                if let Some(c) = &mut self.console {
                    c.open_palette();
                }
            }
            Shortcut::ShowToolbar => {
                if let Some(c) = &mut self.console {
                    c.fullscreen = false;
                }
                if let Some(w) = &self.window {
                    w.set_fullscreen(None);
                }
            }
        }
        self.request_redraw();
    }

    pub(super) fn request_redraw(&mut self) {
        self.egui_dirty = true;
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// Runs egui if needed, then draws video + UI.
    pub(super) fn redraw(&mut self) {
        let (Some(window), Some(gpu), Some(state)) =
            (&self.window, &mut self.gpu, &mut self.egui_state)
        else {
            return;
        };
        if self.egui_dirty || self.egui_repaint_at.is_some_and(|t| t <= Instant::now()) {
            self.egui_dirty = false;
            self.egui_repaint_at = None;
            let input = state.take_egui_input(window);
            let ctx = self.egui_ctx.clone();
            let devices = &mut self.devices;
            let console = &mut self.console;
            let connecting = self.connecting.as_deref();
            let mut connect = None;
            let mut action = ConsoleAction::None;
            let mut output = ctx.run_ui(input, |ui| match console {
                Some(c) => action = c.ui(ui),
                None => connect = devices.ui(ui, connecting),
            });
            state.handle_platform_output(window, std::mem::take(&mut output.platform_output));
            if let Some(vp) = output.viewport_output.get(&egui::ViewportId::ROOT) {
                if vp.repaint_delay.is_zero() {
                    self.egui_dirty = true;
                    window.request_redraw();
                } else if vp.repaint_delay < Duration::from_secs(3600) {
                    self.egui_repaint_at = Some(Instant::now() + vp.repaint_delay);
                }
            }
            gpu.set_egui(&ctx, output);
            if let Some(req) = connect {
                self.start_connect(req);
            }
            self.handle_action(action);
        }
        let ppp = self.pixels_per_point();
        let Some(gpu) = &mut self.gpu else { return };
        let rect = self.console.as_mut().and_then(|c| {
            gpu.video.set_rotation(c.video_rotation());
            c.update_layout(gpu, ppp);
            c.video_rect(gpu, ppp)
        });
        if let Err(e) = gpu.render(rect) {
            tracing::warn!(%e, "render failed");
        }
    }

    pub(super) fn handle_action(&mut self, action: ConsoleAction) {
        match action {
            ConsoleAction::None => {}
            ConsoleAction::Disconnect => {
                if let Some(c) = self.console.take() {
                    c.disconnect();
                }
                if let Some(gpu) = &mut self.gpu {
                    gpu.video.clear();
                }
                if let Some(w) = &self.window {
                    w.set_fullscreen(None);
                }
                self.set_title();
                self.request_redraw();
            }
            ConsoleAction::ToggleFullscreen => self.toggle_fullscreen(),
            ConsoleAction::TargetChanged => {
                if let Some(c) = &self.console {
                    self.devices.update_device(&c.device);
                }
            }
        }
    }
}
