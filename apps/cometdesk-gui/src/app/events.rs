//! winit `ApplicationHandler` implementation and mouse/keyboard/window event routing.
//!
//! Entry point: `<App as ApplicationHandler<UserEvent>>`. Read `window.rs` for input-capture rules.

use std::sync::Arc;
use std::time::Instant;

use cometdesk::hid::MouseButton;
use cometdesk_view::RenderPath;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

#[cfg(target_os = "macos")]
use super::macos::{hide_menu_bar_in_fullscreen, set_dock_icon};
use super::{App, UserEvent};
use crate::console::Console;
use crate::gpu::Gpu;
use crate::theme;

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        #[allow(unused_mut)]
        let mut attrs = Window::default_attributes()
            .with_title("CometDesk")
            .with_inner_size(LogicalSize::new(1280.0, 800.0))
            .with_min_inner_size(LogicalSize::new(1000.0, 600.0));
        // macOS takes the icon from the app bundle (and ignores window icons).
        #[cfg(not(target_os = "macos"))]
        if let Some((rgba, w, h)) = theme::logo_rgba() {
            attrs = attrs.with_window_icon(winit::window::Icon::from_rgba(rgba, w, h).ok());
        }
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                tracing::error!(%e, "cannot create window");
                event_loop.exit();
                return;
            }
        };
        #[cfg(target_os = "macos")]
        {
            hide_menu_bar_in_fullscreen(&window);
            set_dock_icon();
        }
        theme::apply(&self.egui_ctx);
        let gpu = match pollster::block_on(Gpu::new(window.clone(), RenderPath::Auto)) {
            Ok(g) => g,
            Err(e) => {
                tracing::error!(%e, "cannot initialise the GPU");
                event_loop.exit();
                return;
            }
        };
        self.egui_state = Some(egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            Some(gpu.max_texture_side()),
        ));
        self.gpu = Some(gpu);
        window.focus_window();
        self.window = Some(window);
        if let Some(req) = self.devices.autoconnect() {
            self.start_connect(req);
        }
        self.request_redraw();
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::ResumeTimeReached { .. } = cause {
            if let Some(c) = &mut self.console {
                c.tick(Instant::now());
            }
            self.request_redraw();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Control(req) => self.control(event_loop, req),
            UserEvent::Frame => {
                let (Some(console), Some(gpu)) = (&mut self.console, &mut self.gpu) else {
                    return;
                };
                if let Some(ui_changed) = console.on_frame(gpu) {
                    self.egui_dirty |= ui_changed;
                    self.redraw();
                }
            }
            UserEvent::Refresh => self.request_redraw(),
            UserEvent::Notice(text) => {
                if let Some(c) = &mut self.console {
                    c.set_notice(text);
                }
                self.request_redraw();
            }
            UserEvent::Connected(result) => {
                self.connecting = None;
                match *result {
                    Ok(connected) => {
                        let console = Console::new(
                            connected,
                            self.rt.handle().clone(),
                            self.proxy.clone(),
                            self.iso_fix,
                        );
                        self.console = Some(console);
                        self.set_title();
                    }
                    Err((req, e)) => self.devices.connect_failed(req, e),
                }
                self.request_redraw();
            }
            UserEvent::Password(device, password) => {
                if let Some(req) = self.devices.on_password(device, password) {
                    self.start_connect(req);
                }
                self.request_redraw();
            }
            UserEvent::ClipSecret(secret) => {
                if let Some(c) = &mut self.console {
                    c.on_clip_secret(secret);
                }
                self.request_redraw();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match &event {
            WindowEvent::CloseRequested => {
                // Teardown runs in `App::shutdown`, after the event loop returns: a panic
                // inside this winit/AppKit delegate callback cannot unwind and aborts.
                event_loop.exit();
                return;
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
                return;
            }
            WindowEvent::Resized(size) => {
                // Fullscreen can also be toggled outside the app (the macOS green button).
                if let (Some(w), Some(c)) = (&self.window, &mut self.console) {
                    let fullscreen = w.fullscreen().is_some();
                    if c.fullscreen != fullscreen {
                        c.fullscreen = fullscreen;
                        self.egui_dirty = true;
                    }
                }
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size.width, size.height);
                }
            }
            WindowEvent::Occluded(occluded) => {
                if let Some(c) = &mut self.console {
                    c.set_visible(!occluded);
                }
            }
            WindowEvent::Focused(focused) => {
                self.focused = *focused;
                if let Some(c) = &mut self.console {
                    c.set_window_focused(*focused);
                    if !focused {
                        c.release_all();
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event: key,
                is_synthetic,
                ..
            } if self.capturing_keyboard() => {
                // winit synthesizes presses for keys already held when focus arrives.
                if *is_synthetic {
                    return;
                }
                if let (PhysicalKey::Code(code), Some(console)) =
                    (key.physical_key, &mut self.console)
                {
                    let pressed = key.state == ElementState::Pressed;
                    if let Some(s) = console.on_key(code, pressed, key.repeat) {
                        self.shortcut(s);
                    }
                }
                return;
            }
            WindowEvent::ModifiersChanged(_) if self.capturing_keyboard() => return,
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_px = Some((position.x, position.y));
                let mut to_egui = true;
                if let Some(c) = &mut self.console {
                    c.on_cursor((position.x, position.y));
                    // Hovering the video doesn't need egui, except the fullscreen toolbar strip
                    // and one last move after leaving the UI so hover highlights clear.
                    let near_top = position.y / f64::from(self.egui_ctx.pixels_per_point()) <= 8.0;
                    let over_ui = !self.pointer_to_target() || near_top;
                    to_egui = over_ui || self.egui_had_pointer;
                    self.egui_had_pointer = over_ui;
                }
                if !to_egui {
                    return;
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor_px = None;
                if let Some(c) = &mut self.console {
                    c.on_cursor_left();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = *state == ElementState::Pressed;
                let to_target = self.pointer_to_target();
                if let (Some(c), Some(b)) = (&mut self.console, map_button(*button)) {
                    // Releases always go through (the mapper drops unmatched ones).
                    if to_target || !pressed {
                        c.on_button(b, pressed);
                    }
                }
                if to_target {
                    return;
                }
            }
            WindowEvent::MouseWheel { delta, .. } if self.pointer_to_target() => {
                if let Some(c) = &mut self.console {
                    match delta {
                        MouseScrollDelta::LineDelta(x, y) => {
                            c.on_wheel_lines(f64::from(*x), f64::from(*y))
                        }
                        MouseScrollDelta::PixelDelta(p) => {
                            let s = self.window.as_ref().map_or(1.0, |w| w.scale_factor());
                            c.on_wheel_pixels(p.x / s, p.y / s);
                        }
                    }
                }
                return;
            }
            _ => {}
        }
        let (Some(window), Some(state)) = (&self.window, &mut self.egui_state) else {
            return;
        };
        let response = state.on_window_event(window, &event);
        if response.repaint {
            self.request_redraw();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let console_due = self.console.as_mut().and_then(|c| c.tick(now));
        let due = [console_due, self.egui_repaint_at]
            .into_iter()
            .flatten()
            .min();
        event_loop.set_control_flow(match due {
            Some(t) => ControlFlow::WaitUntil(t),
            None => ControlFlow::Wait,
        });
    }
}

fn map_button(b: winit::event::MouseButton) -> Option<MouseButton> {
    Some(match b {
        winit::event::MouseButton::Left => MouseButton::Left,
        winit::event::MouseButton::Right => MouseButton::Right,
        winit::event::MouseButton::Middle => MouseButton::Middle,
        winit::event::MouseButton::Back => MouseButton::Back,
        winit::event::MouseButton::Forward => MouseButton::Forward,
        winit::event::MouseButton::Other(_) => return None,
    })
}
