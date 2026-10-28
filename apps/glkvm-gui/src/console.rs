//! A connected device: video, input, toolbar and status.
//!
//! Power rule (PLAN.md §3): nothing renders continuously. While the window is hidden,
//! decoding stops at once; after 30 s the video stream is dropped and the event socket
//! switches to `stream=0`, so the device can stop encoding. Showing the window resumes both.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use glkvm::api::atx::{AtxButton, PowerAction};
use glkvm::api::capabilities::{Capabilities, Feature};
use glkvm::api::hid::PrintOptions;
use glkvm::api::streamer::SnapshotOptions;
use glkvm::events::{ConnState, DeviceState, EventOptions, EventSession, HidSender};
use glkvm::hid::{HidCommand, MouseButton};
use glkvm::{Session, SystemConfig};
use glkvm_media::direct::{self, DirectOptions, MediaSession, SourceState};
use glkvm_view::pipeline::FramePipeline;
use glkvm_view::{InputMapper, Rect, ScrollOptions, letterbox_in};
use tokio::runtime::Handle;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use winit::event_loop::EventLoopProxy;
use winit::keyboard::KeyCode;

use crate::actions::{self, HostModifiers, Shortcut};
use crate::app::UserEvent;
use crate::clipsync::{BridgeState, ClipBridge, Settings as ClipSettings};
use crate::gpu::Gpu;
use crate::store::{self, ClipDirection, SavedDevice, TargetOs};

const HIDDEN_GRACE: Duration = Duration::from_secs(30);
const NOTICE_TIME: Duration = Duration::from_secs(4);

/// Everything the connect task produces.
pub struct Connected {
    pub device: SavedDevice,
    pub session: Session,
    pub caps: Capabilities,
    pub config: SystemConfig,
    /// Keyboard layouts the device can type with.
    pub keymaps: Vec<String>,
}

struct Stream {
    media: MediaSession,
    pipeline: FramePipeline,
}

struct Typing {
    total: usize,
    done: Arc<AtomicUsize>,
    task: JoinHandle<Result<(), String>>,
}

enum Confirm {
    Type { text: String, reason: String },
    Power(PowerAction),
    Click(AtxButton),
}

/// What the UI asks the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleAction {
    None,
    Disconnect,
    ToggleFullscreen,
    TargetChanged,
}

#[derive(Default)]
struct FrameStats {
    window_start: Option<Instant>,
    frames: u32,
    net_to_screen: Duration,
    decode: Duration,
    shown: String,
}

pub struct Console {
    pub device: SavedDevice,
    session: Session,
    caps: Capabilities,
    /// Device default layout (from its config), used when the saved device has none.
    default_keymap: Option<String>,
    keymaps: Vec<String>,
    rt: Handle,
    proxy: EventLoopProxy<UserEvent>,
    events: Option<EventSession>,
    hid: HidSender,
    conn: watch::Receiver<ConnState>,
    state: watch::Receiver<DeviceState>,
    stream: Option<Stream>,
    watcher: Option<JoinHandle<()>>,
    mapper: InputMapper,
    mods: HostModifiers,
    cmds: Vec<HidCommand>,
    pub fullscreen: bool,
    pub show_stats: bool,
    toolbar_hover: bool,
    /// Toolbar rectangle in points while it is shown.
    toolbar_rect: Option<egui::Rect>,
    typing: Option<Typing>,
    confirm: Option<Confirm>,
    notice: Option<(String, Instant)>,
    stats: FrameStats,
    hidden_since: Option<Instant>,
    /// Area for the video, in logical points (below the toolbar).
    video_area: egui::Rect,
    /// Clipboard bridge to the agent on the target; `None` until paired.
    clip: Option<ClipBridge>,
    clip_paused: bool,
    /// Pairing code shown in the pairing dialog while it is open.
    pairing: Option<String>,
}

impl Console {
    pub fn new(c: Connected, rt: Handle, proxy: EventLoopProxy<UserEvent>, iso_fix: bool) -> Self {
        let scroll =
            ScrollOptions::from_config(c.config.scroll_rate, c.config.reverse_scrolling.as_deref());
        let mapper = InputMapper::new(c.device.target.key_options(iso_fix), scroll);
        let _guard = rt.enter();
        let events = EventSession::spawn(c.session.clone(), EventOptions::default());
        let mut console = Console {
            hid: events.hid(),
            conn: events.conn_state(),
            state: events.device_state(),
            events: Some(events),
            device: c.device,
            session: c.session,
            caps: c.caps,
            default_keymap: c.config.keymap,
            keymaps: c.keymaps,
            rt,
            proxy,
            stream: None,
            watcher: None,
            mapper,
            mods: HostModifiers::default(),
            cmds: Vec::with_capacity(16),
            fullscreen: false,
            show_stats: false,
            toolbar_hover: false,
            toolbar_rect: None,
            typing: None,
            confirm: None,
            notice: None,
            stats: FrameStats::default(),
            hidden_since: None,
            video_area: egui::Rect::NOTHING,
            clip: None,
            clip_paused: false,
            pairing: None,
        };
        console.start_stream();
        if let Some(secret) = store::clip_secret(&console.device) {
            console.start_clip(secret);
        }
        console
    }

    // ----- clipboard bridge ----------------------------------------------------------------

    fn clip_settings(&self) -> ClipSettings {
        ClipSettings {
            direction: self.device.clip_direction,
            paused: self.clip_paused,
        }
    }

    fn start_clip(&mut self, secret: glkvm_clip::Secret) {
        let proxy = self.proxy.clone();
        self.clip = Some(ClipBridge::spawn(
            &self.rt,
            self.session.clone(),
            secret,
            self.clip_settings(),
            move || {
                let _ = proxy.send_event(UserEvent::Refresh);
            },
        ));
    }

    /// New pairing code: replaces any old pairing and opens the pairing dialog.
    fn pair_clipboard(&mut self) {
        let secret = glkvm_clip::Secret::generate();
        if let Err(e) = store::set_clip_secret(&self.device, &secret) {
            return self.notify(format!("Cannot store the pairing code: {e}"));
        }
        self.pairing = Some(secret.to_code());
        self.start_clip(secret);
    }

    fn forget_clipboard_pairing(&mut self) {
        store::delete_clip_secret(&self.device);
        self.clip = None;
        self.notify("Clipboard pairing removed".into());
    }

    fn clip_menu(&mut self, ui: &mut egui::Ui) -> ConsoleAction {
        let mut action = ConsoleAction::None;
        let label = match self.clip.as_ref().map(ClipBridge::state) {
            None => "Clipboard: off".to_string(),
            Some(_) if self.clip_paused => "Clipboard: paused".into(),
            Some(BridgeState::Connected(_)) => "Clipboard: synced".into(),
            Some(_) => "Clipboard: not connected".into(),
        };
        ui.menu_button(label, |ui| {
            let Some(state) = self.clip.as_ref().map(ClipBridge::state) else {
                ui.label("Shares the clipboard with glkvm-agent on the target.");
                if ui.button("Pair agent…").clicked() {
                    self.pair_clipboard();
                    ui.close();
                }
                return;
            };
            ui.label(state.label());
            ui.separator();
            let before = self.device.clip_direction;
            for d in ClipDirection::ALL {
                ui.radio_value(&mut self.device.clip_direction, d, d.label());
            }
            if self.device.clip_direction != before {
                action = ConsoleAction::TargetChanged;
            }
            ui.checkbox(&mut self.clip_paused, "Pause");
            if let Some(c) = &self.clip
                && c.settings() != self.clip_settings()
            {
                c.set_settings(self.clip_settings());
            }
            ui.separator();
            if ui.button("Pair again (new code)…").clicked() {
                self.pair_clipboard();
                ui.close();
            }
            if ui.button("Forget pairing").clicked() {
                self.forget_clipboard_pairing();
                ui.close();
            }
        });
        action
    }

    fn pairing_dialog(&mut self, ctx: &egui::Context) {
        let Some(code) = self.pairing.clone() else {
            return;
        };
        let command = format!("glkvm-agent pair {code}");
        let state = self.clip.as_ref().map(ClipBridge::state);
        let mut close = false;
        egui::Modal::new(egui::Id::unique("clip-pairing")).show(ctx, |ui| {
            ui.heading("Pair the clipboard agent");
            ui.label("On the target, in a terminal, run:");
            ui.add(
                egui::TextEdit::singleline(&mut command.as_str())
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY),
            );
            ui.label("then start it with `glkvm-agent run`.");
            ui.horizontal(|ui| {
                if ui
                    .button("Type it on the target")
                    .on_hover_text("Types the command (without Enter) into the focused window")
                    .clicked()
                {
                    self.start_typing(command.clone());
                }
                if ui.button("Copy").clicked() {
                    ui.ctx().copy_text(command.clone());
                }
            });
            if let Some(s) = &state {
                ui.label(format!("Status: {}", s.label()));
            }
            if ui.button("Done").clicked() {
                close = true;
            }
        });
        if close {
            self.pairing = None;
        }
    }

    fn start_stream(&mut self) {
        let _guard = self.rt.enter();
        let media = direct::start(self.session.clone(), DirectOptions::default());
        let (video, media) = media.take_video();
        let proxy = self.proxy.clone();
        match FramePipeline::spawn(video, media.control.clone(), move || {
            let _ = proxy.send_event(UserEvent::Frame);
        }) {
            Ok(pipeline) => self.stream = Some(Stream { media, pipeline }),
            Err(e) => self.notify(format!("cannot start decoder: {e}")),
        }
        self.restart_watcher();
    }

    /// Wakes the UI when something shown in the toolbar changes (not on every event).
    fn restart_watcher(&mut self) {
        if let Some(w) = self.watcher.take() {
            w.abort();
        }
        let mut conn = self.conn.clone();
        let mut state = self.state.clone();
        let mut media = self.stream.as_ref().map(|s| s.media.state.clone());
        let proxy = self.proxy.clone();
        self.watcher = Some(self.rt.spawn(async move {
            let key = |c: &ConnState, s: &DeviceState, m: Option<&SourceState>| {
                let hid_online = s.hid().map(|h| h.online);
                (
                    c.clone(),
                    s.video_signal(),
                    hid_online,
                    m.map(std::mem::discriminant),
                )
            };
            let mut last = None;
            loop {
                let current = key(
                    &conn.borrow(),
                    &state.borrow(),
                    media.as_ref().map(|m| m.borrow().clone()).as_ref(),
                );
                if last.as_ref() != Some(&current) {
                    last = Some(current);
                    if proxy.send_event(UserEvent::Refresh).is_err() {
                        return;
                    }
                }
                let media_changed = async {
                    match media.as_mut() {
                        Some(m) => m.changed().await.is_ok(),
                        None => std::future::pending().await,
                    }
                };
                tokio::select! {
                    r = conn.changed() => if r.is_err() { return },
                    r = state.changed() => if r.is_err() { return },
                    ok = media_changed => if !ok { return },
                }
            }
        }));
    }

    fn notify(&mut self, text: String) {
        tracing::info!("{text}");
        self.notice = Some((text, Instant::now()));
    }

    pub fn set_notice(&mut self, text: String) {
        self.notify(text);
    }

    // ----- video -------------------------------------------------------------------------

    /// Takes a decoded frame into the renderer. `None`: nothing new. `Some(true)`: new frame
    /// and the UI changed too (first frame, or the stats line was updated).
    pub fn on_frame(&mut self, gpu: &mut Gpu) -> Option<bool> {
        let decoded = self.stream.as_ref()?.pipeline.take()?;
        if !gpu.video.set_frame(&decoded.frame) {
            return None;
        }
        let first = self.stats.window_start.is_none() && self.stats.shown.is_empty();
        let now = Instant::now();
        let s = &mut self.stats;
        let start = *s.window_start.get_or_insert(now);
        s.frames += 1;
        s.net_to_screen += now - decoded.received_at;
        s.decode += decoded.decoded_at - decoded.received_at;
        let secs = (now - start).as_secs_f64();
        if secs >= 1.0 {
            let n = f64::from(s.frames);
            let media = self
                .stream
                .as_ref()
                .map(|st| *st.media.stats.borrow())
                .unwrap_or_default();
            let replaced = self.stream.as_ref().map_or(0, |st| st.pipeline.replaced());
            s.shown = format!(
                "{} fps · {:.1} Mb/s · decode {:.1} ms · net→screen {:.1} ms · dropped {} · {}",
                (n / secs).round(),
                media.kbps / 1000.0,
                s.decode.as_secs_f64() * 1000.0 / n,
                s.net_to_screen.as_secs_f64() * 1000.0 / n,
                media.dropped_total + replaced,
                gpu.video.path_name(),
            );
            *s = FrameStats {
                shown: std::mem::take(&mut s.shown),
                ..Default::default()
            };
            s.window_start = Some(now);
            return Some(self.show_stats || first);
        }
        Some(first)
    }

    /// Video rectangle in physical pixels, letterboxed inside the video area.
    pub fn video_rect(&self, gpu: &Gpu, pixels_per_point: f32) -> Option<Rect> {
        let video = gpu.video.video_size()?;
        if !self.video_area.is_positive() {
            return None;
        }
        let ppp = f64::from(pixels_per_point);
        let a = self.video_area;
        let area = Rect {
            x: f64::from(a.min.x) * ppp,
            y: f64::from(a.min.y) * ppp,
            w: f64::from(a.width()) * ppp,
            h: f64::from(a.height()) * ppp,
        };
        Some(letterbox_in(area, video))
    }

    /// Keeps the input mapping in sync with where the video is drawn.
    pub fn update_layout(&mut self, gpu: &Gpu, pixels_per_point: f32) {
        if let Some(rect) = self.video_rect(gpu, pixels_per_point) {
            self.mapper.set_rect(rect);
        }
    }

    // ----- visibility ------------------------------------------------------------------

    pub fn set_visible(&mut self, visible: bool) {
        if visible {
            self.hidden_since = None;
            match &self.stream {
                Some(s) => s.pipeline.set_visible(true),
                None => self.resume(),
            }
        } else {
            self.hidden_since.get_or_insert_with(Instant::now);
            if let Some(s) = &self.stream {
                s.pipeline.set_visible(false);
            }
        }
    }

    /// Call periodically; returns when it wants to be called again.
    pub fn tick(&mut self, now: Instant) -> Option<Instant> {
        if let Some((_, t)) = &self.notice
            && now - *t > NOTICE_TIME
        {
            self.notice = None;
        }
        let mut next = self.notice.as_ref().map(|(_, t)| *t + NOTICE_TIME);
        if let Some(since) = self.hidden_since
            && self.stream.is_some()
        {
            if now - since >= HIDDEN_GRACE {
                self.suspend();
            } else {
                let due = since + HIDDEN_GRACE;
                next = Some(next.map_or(due, |n| n.min(due)));
            }
        }
        next
    }

    fn suspend(&mut self) {
        tracing::info!("hidden for {HIDDEN_GRACE:?}: dropping the video stream");
        self.stream = None;
        self.reopen_events(false);
    }

    fn resume(&mut self) {
        tracing::info!("visible again: resuming the video stream");
        self.reopen_events(true);
        self.start_stream();
    }

    fn reopen_events(&mut self, stream: bool) {
        let _guard = self.rt.enter();
        if let Some(old) = self.events.take() {
            self.rt.spawn(old.close());
        }
        let events = EventSession::spawn(
            self.session.clone(),
            EventOptions {
                stream,
                ..Default::default()
            },
        );
        self.hid = events.hid();
        self.conn = events.conn_state();
        self.state = events.device_state();
        self.events = Some(events);
        self.restart_watcher();
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

    pub fn has_modal(&self) -> bool {
        self.confirm.is_some() || self.pairing.is_some()
    }

    pub fn send_combo(&mut self, keys: &[&'static str]) {
        for cmd in actions::combo(keys) {
            self.hid.send(cmd);
        }
    }

    // ----- actions ---------------------------------------------------------------------

    pub fn request_type_clipboard(&mut self) {
        let text = match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(t) if !t.is_empty() => t,
            Ok(_) => return self.notify("The clipboard has no text".into()),
            Err(e) => return self.notify(format!("Cannot read the clipboard: {e}")),
        };
        // The user may still hold Cmd+Shift from the shortcut; those must not reach the target
        // together with the typed text.
        self.release_all();
        match actions::confirm_reason(&text) {
            Some(reason) => self.confirm = Some(Confirm::Type { text, reason }),
            None => self.start_typing(text),
        }
    }

    fn start_typing(&mut self, text: String) {
        if let Some(t) = self.typing.take() {
            t.task.abort();
        }
        let chunks: Vec<String> = actions::chunks(&text, actions::CHUNK_CHARS)
            .into_iter()
            .map(str::to_owned)
            .collect();
        let total = text.chars().count();
        let done = Arc::new(AtomicUsize::new(0));
        let (session, keymap, proxy, progress) = (
            self.session.clone(),
            self.keymap(),
            self.proxy.clone(),
            done.clone(),
        );
        let task = self.rt.spawn(async move {
            for chunk in chunks {
                let opts = PrintOptions {
                    keymap: keymap.as_deref(),
                    slow: false,
                };
                session
                    .hid_print(&chunk, opts)
                    .await
                    .map_err(|e| e.to_string())?;
                progress.fetch_add(chunk.chars().count(), Ordering::Relaxed);
                let _ = proxy.send_event(UserEvent::Refresh);
            }
            Ok(())
        });
        self.typing = Some(Typing { total, done, task });
    }

    /// Layout used to type text: the device's saved choice, else the device default.
    fn keymap(&self) -> Option<String> {
        self.device
            .keymap
            .clone()
            .or_else(|| self.default_keymap.clone())
    }

    /// Stops typing: dropping the request makes the device cancel and clear queued keys.
    fn cancel_typing(&mut self) {
        if let Some(t) = self.typing.take() {
            t.task.abort();
            self.notify("Typing cancelled".into());
        }
    }

    pub fn snapshot(&mut self) {
        let session = self.session.clone();
        let proxy = self.proxy.clone();
        let host = self.device.host.replace(':', "_");
        self.rt.spawn(async move {
            let msg = match session.snapshot(SnapshotOptions::default()).await {
                Ok(jpeg) => {
                    let dir = std::env::var_os("HOME")
                        .map(|h| std::path::PathBuf::from(h).join("Downloads"))
                        .unwrap_or_else(std::env::temp_dir);
                    let stamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs());
                    let path = dir.join(format!("glkvm-{host}-{stamp}.jpg"));
                    match std::fs::write(&path, &jpeg) {
                        Ok(()) => format!("Snapshot saved to {}", path.display()),
                        Err(e) => format!("Cannot save snapshot: {e}"),
                    }
                }
                Err(e) => format!("Snapshot failed: {e}"),
            };
            let _ = proxy.send_event(UserEvent::Notice(msg));
        });
    }

    fn atx(&mut self, confirm: Confirm) {
        let session = self.session.clone();
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            let (label, r) = match confirm {
                Confirm::Power(a) => (a.as_str(), session.atx_power(a, true).await),
                Confirm::Click(b) => (b.as_str(), session.atx_click(b, true).await),
                Confirm::Type { .. } => return,
            };
            let msg = match r {
                Ok(()) => format!("ATX {label}: done"),
                Err(e) => format!("ATX {label} failed: {e}"),
            };
            let _ = proxy.send_event(UserEvent::Notice(msg));
        });
    }

    pub fn disconnect(mut self) {
        self.release_all();
        if let Some(w) = self.watcher.take() {
            w.abort();
        }
        self.stream = None;
        self.clip = None;
        if let Some(t) = self.typing.take() {
            t.task.abort();
        }
        let session = self.session.clone();
        let events = self.events.take();
        self.rt.spawn(async move {
            if let Some(e) = events {
                e.close().await;
            }
            let _ = session.logout().await;
        });
    }

    // ----- ui --------------------------------------------------------------------------

    /// Whether the toolbar is drawn this frame (always, except in fullscreen).
    pub fn toolbar_visible(&self, pointer: Option<egui::Pos2>) -> bool {
        !self.fullscreen
            || self.toolbar_hover
            || pointer.is_some_and(|p| p.y <= 6.0)
            || self.confirm.is_some()
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> ConsoleAction {
        let mut action = ConsoleAction::None;
        let pointer = ui.ctx().pointer_latest_pos();
        if self.toolbar_visible(pointer) {
            let resp = egui::Panel::top("toolbar").show(ui, |ui| {
                ui.horizontal_wrapped(|ui| action = self.toolbar(ui));
            });
            self.toolbar_hover = self.fullscreen && resp.response.contains_pointer();
            self.toolbar_rect = Some(resp.response.rect);
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
        self.dialogs(ui.ctx());
        action
    }

    fn status(&self) -> (String, egui::Color32) {
        let red = egui::Color32::from_rgb(220, 80, 70);
        let amber = egui::Color32::from_rgb(230, 170, 50);
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
        ("Connected".into(), egui::Color32::from_rgb(90, 180, 110))
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) -> ConsoleAction {
        let mut action = ConsoleAction::None;
        if ui.button("< Devices").clicked() {
            action = ConsoleAction::Disconnect;
        }
        ui.strong(self.device.display_name());
        let (text, color) = self.status();
        ui.colored_label(color, text);
        ui.separator();

        ui.menu_button("Keys", |ui| {
            for (label, keys) in actions::COMBOS {
                if ui.button(*label).clicked() {
                    self.send_combo(keys);
                    ui.close();
                }
            }
            ui.menu_button("Function keys", |ui| {
                for key in actions::FUNCTION_KEYS {
                    if ui.button(key).clicked() {
                        self.send_combo(&[key]);
                        ui.close();
                    }
                }
            });
        });
        if ui
            .button("Type clipboard")
            .on_hover_text("Right Cmd + V")
            .clicked()
        {
            self.request_type_clipboard();
        }
        if self.clip_menu(ui) != ConsoleAction::None {
            action = ConsoleAction::TargetChanged;
        }
        if ui
            .button("Snapshot")
            .on_hover_text("Saved to ~/Downloads")
            .clicked()
        {
            self.snapshot();
        }
        if self.caps.supports(Feature::Atx) {
            ui.menu_button("Power", |ui| {
                let items = [
                    ("Power on", Confirm::Power(PowerAction::On)),
                    ("Shut down", Confirm::Power(PowerAction::Off)),
                    ("Force off", Confirm::Power(PowerAction::OffHard)),
                    ("Reset", Confirm::Power(PowerAction::ResetHard)),
                    ("Press power button", Confirm::Click(AtxButton::Power)),
                ];
                for (label, c) in items {
                    if ui.button(label).clicked() {
                        self.confirm = Some(c);
                        ui.close();
                    }
                }
            });
        }
        ui.separator();
        let before = self.device.target;
        egui::ComboBox::from_id_salt("target")
            .selected_text(format!("Target: {}", self.device.target.label()))
            .show_ui(ui, |ui| {
                for os in TargetOs::ALL {
                    ui.selectable_value(&mut self.device.target, os, os.label());
                }
            });
        if self.device.target != before {
            let iso = self.mapper.keys.iso_fix;
            self.mapper.keys = self.device.target.key_options(iso);
            action = ConsoleAction::TargetChanged;
        }
        let current = self.keymap().unwrap_or_else(|| "default".into());
        let before = self.device.keymap.clone();
        egui::ComboBox::from_id_salt("keymap")
            .selected_text(format!("Layout: {current}"))
            .show_ui(ui, |ui| {
                for k in &self.keymaps {
                    ui.selectable_value(&mut self.device.keymap, Some(k.clone()), k.as_str());
                }
            })
            .response
            .on_hover_text("Keyboard layout of the target, used by Type clipboard");
        if self.device.keymap != before {
            action = ConsoleAction::TargetChanged;
        }
        ui.toggle_value(&mut self.show_stats, "Stats")
            .on_hover_text("Right Cmd + S");
        if ui
            .button(if self.fullscreen {
                "Exit fullscreen"
            } else {
                "Fullscreen"
            })
            .on_hover_text("Right Cmd + F")
            .clicked()
        {
            action = ConsoleAction::ToggleFullscreen;
        }
        if ui
            .button("Device settings...")
            .on_hover_text("Opens the device's web interface")
            .clicked()
        {
            let url = format!("https://{}/", self.session.host());
            if let Err(e) = open::that(&url) {
                self.notify(format!("Cannot open {url}: {e}"));
            }
        }
        action
    }

    fn overlays(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let (status, _) = self.status();
        let center_msg = match status.as_str() {
            "No signal" => Some("No signal: the target is off or asleep"),
            "Reconnecting…" => Some("Reconnecting…"),
            "Paused (window hidden)" => Some("Resuming…"),
            _ if self.stats.window_start.is_none() && self.stats.shown.is_empty() => {
                Some("Waiting for video…")
            }
            _ => None,
        };
        if let Some(msg) = center_msg {
            ui.put(
                rect,
                egui::Label::new(
                    egui::RichText::new(msg)
                        .size(18.0)
                        .color(egui::Color32::LIGHT_GRAY),
                ),
            );
        }
        if self.show_stats && !self.stats.shown.is_empty() {
            egui::Area::new(egui::Id::unique("stats"))
                .anchor(egui::Align2::RIGHT_BOTTOM, [-8.0, -8.0])
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| ui.monospace(&self.stats.shown));
                });
        }
        if let Some(typing) = &self.typing {
            let done = typing.done.load(Ordering::Relaxed);
            let mut cancel = false;
            egui::Area::new(egui::Id::unique("typing"))
                .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -16.0])
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::ProgressBar::new(done as f32 / typing.total.max(1) as f32)
                                    .desired_width(220.0)
                                    .text(format!("Typing {done}/{}", typing.total)),
                            );
                            cancel = ui.button("Cancel").clicked();
                        });
                    });
                });
            if cancel {
                self.cancel_typing();
            } else if typing.task.is_finished() {
                let task = self.typing.take().expect("checked above").task;
                let result = self.rt.block_on(task);
                match result {
                    Ok(Ok(())) => self.notify("Typed the clipboard".into()),
                    Ok(Err(e)) => self.notify(format!("Typing failed: {e}")),
                    Err(_) => {}
                }
            }
        }
        if let Some((text, _)) = &self.notice {
            egui::Area::new(egui::Id::unique("notice"))
                .anchor(egui::Align2::CENTER_TOP, [0.0, 48.0])
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(text));
                });
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        self.pairing_dialog(ctx);
        let Some(confirm) = &self.confirm else { return };
        let (title, body) = match confirm {
            Confirm::Type { reason, .. } => (
                "Type the clipboard on the target?".to_string(),
                format!("The text has {reason}."),
            ),
            Confirm::Power(a) => (
                format!("ATX power: {}?", a.as_str()),
                "This acts on the target's power button.".into(),
            ),
            Confirm::Click(b) => (
                format!("Press the ATX {} button?", b.as_str()),
                "This acts on the target directly.".into(),
            ),
        };
        let mut decision = None;
        egui::Modal::new(egui::Id::unique("confirm")).show(ctx, |ui| {
            ui.heading(title);
            ui.label(body);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    decision = Some(false);
                }
                if ui.button("Continue").clicked() {
                    decision = Some(true);
                }
            });
        });
        match decision {
            Some(true) => match self.confirm.take() {
                Some(Confirm::Type { text, .. }) => self.start_typing(text),
                Some(c) => self.atx(c),
                None => {}
            },
            Some(false) => self.confirm = None,
            None => {}
        }
    }
}
