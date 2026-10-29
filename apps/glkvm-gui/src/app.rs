//! Window, event loop and screen switching.
//!
//! Input routing: while a console is focused and no dialog or text field wants the keyboard,
//! key events go to the target, not to egui. Pointer events over the video go to the target;
//! over the toolbar or dialogs they go to egui. Releases always reach the target so nothing
//! stays pressed.

use std::sync::Arc;
use std::time::{Duration, Instant};

use glkvm::hid::MouseButton;
use glkvm::tls::PinStore;
use glkvm::{Credentials, Error, Session};
use glkvm_view::RenderPath;
use tokio::runtime::Runtime;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::PhysicalKey;
use winit::window::{Fullscreen, Window, WindowId};

use crate::actions::Shortcut;
use crate::console::{Connected, Console, ConsoleAction};
use crate::devices::{ConnectError, ConnectRequest, DevicesUi};
use crate::gpu::Gpu;
use crate::store::{SavedDevice, Store};
use crate::theme;

pub mod control;

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
    ClipSecret(glkvm_clip::Secret),
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

    fn pixels_per_point(&self) -> f32 {
        self.egui_ctx.pixels_per_point()
    }

    /// Keyboard goes to the target (not egui).
    fn capturing_keyboard(&self) -> bool {
        self.focused
            && self.console.as_ref().is_some_and(|c| !c.has_modal())
            && !self.egui_ctx.egui_wants_keyboard_input()
    }

    /// The pointer is over the video, not over toolbar/dialogs.
    fn pointer_to_target(&self) -> bool {
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

    fn start_connect(&mut self, req: ConnectRequest) {
        self.connecting = Some(format!("Connecting to {}…", req.device.display_name()));
        let proxy = self.proxy.clone();
        let pins = self.pins.clone();
        self.rt.spawn(async move {
            let result = connect(&req, pins).await.map_err(|e| (req, e));
            let _ = proxy.send_event(UserEvent::Connected(Box::new(result)));
        });
    }

    fn set_title(&self) {
        if let Some(w) = &self.window {
            match &self.console {
                Some(c) => w.set_title(&format!("glkvm — {}", c.device.display_name())),
                None => w.set_title("glkvm"),
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

    fn shortcut(&mut self, s: Shortcut) {
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

    fn request_redraw(&mut self) {
        self.egui_dirty = true;
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// Runs egui if needed, then draws video + UI.
    fn redraw(&mut self) {
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

    fn handle_action(&mut self, action: ConsoleAction) {
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

async fn connect(req: &ConnectRequest, pins: Arc<PinStore>) -> Result<Connected, ConnectError> {
    let mut creds = Credentials::new(req.device.user.clone(), req.password.clone());
    creds.totp = req.totp.clone();
    let session = Session::login(&req.device.host, &creds, pins)
        .await
        .map_err(|e| match e {
            Error::Forbidden => ConnectError::WrongPassword,
            Error::TotpRequired => ConnectError::NeedTotp,
            e => ConnectError::Other(e.to_string()),
        })?;
    let (caps, config, keymaps) = tokio::join!(
        session.capabilities(),
        session.system_config(),
        session.hid_keymaps()
    );
    let caps = caps.map_err(|e| ConnectError::Other(e.to_string()))?;
    Ok(Connected {
        device: req.device.clone(),
        session,
        caps,
        config: config.unwrap_or_default(),
        keymaps: keymaps.map(|k| k.available).unwrap_or_default(),
    })
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        #[allow(unused_mut)]
        let mut attrs = Window::default_attributes()
            .with_title("glkvm")
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
                if let Some(c) = self.console.take() {
                    let logout = c.disconnect();
                    // Wait for the logout, but no longer than this; the runtime is dropped
                    // with the app.
                    let _ = self
                        .rt
                        .block_on(tokio::time::timeout(Duration::from_millis(500), logout));
                }
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

/// In fullscreen macOS slides its menu bar down at the top edge, over the toolbar that appears
/// there too. The options macOS applies for a native fullscreen window come from the window
/// delegate (winit's, which asks for an auto-hiding menu bar; setting them on the app instead
/// is overwritten), so replace its answer with one that hides the menu bar.
#[cfg(target_os = "macos")]
fn hide_menu_bar_in_fullscreen(window: &Window) {
    use objc2::ffi::class_replaceMethod;
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::{msg_send, sel};
    use objc2_app_kit::NSApplicationPresentationOptions as Opts;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    unsafe extern "C" fn options(_: *mut AnyObject, _: Sel, _: *mut AnyObject, _: usize) -> usize {
        (Opts::FullScreen | Opts::HideDock | Opts::HideMenuBar).bits()
    }

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(h) = handle.as_raw() else {
        return;
    };
    // SAFETY: `ns_view` is the live NSView of `window`; it, its window and the delegate are
    // only messaged here, on the main thread, and the replacement matches the delegate
    // method's signature (`NSApplicationPresentationOptions` and `NSUInteger` are both usize).
    unsafe {
        let view = h.ns_view.as_ptr().cast::<AnyObject>();
        let ns_window: *mut AnyObject = msg_send![view, window];
        if ns_window.is_null() {
            return;
        }
        let delegate: *mut AnyObject = msg_send![ns_window, delegate];
        if delegate.is_null() {
            return;
        }
        let class: *const AnyClass = (*delegate).class();
        let imp: Imp = std::mem::transmute(
            options as unsafe extern "C" fn(*mut AnyObject, Sel, *mut AnyObject, usize) -> usize,
        );
        class_replaceMethod(
            class.cast_mut(),
            sel!(window:willUseFullScreenPresentationOptions:),
            imp,
            c"Q@:@Q".as_ptr(),
        );
    }
}

/// Shows the logo in the Dock when running outside an app bundle (a bundle supplies its own).
#[cfg(target_os = "macos")]
fn set_dock_icon() {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};

    let (Some(nsimage), Some(nsdata), Some(nsapp)) = (
        AnyClass::get(c"NSImage"),
        AnyClass::get(c"NSData"),
        AnyClass::get(c"NSApplication"),
    ) else {
        return;
    };
    let png = theme::LOGO_PNG;
    // SAFETY: plain AppKit calls on the main thread (`resumed` runs there); `dataWithBytes`
    // copies the bytes, and the returned objects are autoreleased or owned by AppKit.
    unsafe {
        let data: *mut AnyObject = msg_send![nsdata, dataWithBytes: png.as_ptr().cast::<std::ffi::c_void>(), length: png.len()];
        let image: *mut AnyObject = msg_send![nsimage, alloc];
        let image: *mut AnyObject = msg_send![image, initWithData: data];
        if image.is_null() {
            return;
        }
        let app: *mut AnyObject = msg_send![nsapp, sharedApplication];
        let _: () = msg_send![app, setApplicationIconImage: image];
    }
}
