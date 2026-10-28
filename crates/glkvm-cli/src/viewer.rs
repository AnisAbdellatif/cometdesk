//! The console window: video from the Direct source, input to the device.
//!
//! Threads:
//! - main: winit event loop, input mapping, upload + render (only when a frame arrives);
//! - tokio runtime: event socket (HID) and media socket;
//! - `decode`: see `glkvm_view::pipeline` (newest frame wins, nothing queues up).

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use glkvm::events::{ConnState, HidSender};
use glkvm::hid::{HidCommand, MouseButton};
use glkvm_media::direct::{MediaSession, MediaStats, SourceState};
use tokio::sync::watch;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

use crate::renderer::Renderer;
use glkvm_view::InputMapper;
use glkvm_view::RenderPath;
use glkvm_view::pipeline::FramePipeline;

#[derive(Debug)]
pub enum UserEvent {
    Frame,
}

pub struct ViewerParts {
    pub media: MediaSession,
    pub hid: HidSender,
    pub conn: watch::Receiver<ConnState>,
    pub mapper: InputMapper,
    pub title: String,
    pub render: RenderPath,
    /// Measure for this long (after a 3 s warm-up), print the result and exit.
    pub bench: Option<Duration>,
}

pub fn run(parts: ViewerParts) -> Result<()> {
    let event_loop = build_event_loop()?;
    let proxy = event_loop.create_proxy();
    let (video, media) = parts.media.take_video();
    let pipeline = FramePipeline::spawn(video, media.control.clone(), move || {
        let _ = proxy.send_event(UserEvent::Frame);
    })?;

    let mut app = App {
        window: None,
        renderer: None,
        hid: parts.hid,
        conn: parts.conn,
        mapper: parts.mapper,
        pipeline,
        media_stats: media.stats.clone(),
        media_state: media.state.clone(),
        _media: media,
        title: parts.title,
        cmds: Vec::with_capacity(16),
        scale_factor: 1.0,
        stats: ViewStats::new(),
        render: parts.render,
        bench: parts.bench.map(Bench::new),
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

fn build_event_loop() -> Result<EventLoop<UserEvent>> {
    let mut builder = EventLoop::<UserEvent>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        // No default app menu: Cmd+Q, Cmd+H, ... go to the target, not to us.
        builder.with_default_menu(false);
    }
    Ok(builder.build()?)
}

struct ViewStats {
    window_start: Instant,
    frames: u32,
    net_to_screen: Duration,
    decode: Duration,
}

impl ViewStats {
    fn new() -> Self {
        Self {
            window_start: Instant::now(),
            frames: 0,
            net_to_screen: Duration::ZERO,
            decode: Duration::ZERO,
        }
    }
}

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    hid: HidSender,
    conn: watch::Receiver<ConnState>,
    mapper: InputMapper,
    pipeline: FramePipeline,
    media_stats: watch::Receiver<MediaStats>,
    media_state: watch::Receiver<SourceState>,
    /// Keeps the media task alive (it is aborted on drop).
    _media: MediaSession,
    title: String,
    cmds: Vec<HidCommand>,
    scale_factor: f64,
    stats: ViewStats,
    render: RenderPath,
    bench: Option<Bench>,
}

/// CPU time used by this process so far (user + system, all threads).
fn process_cpu_time() -> Duration {
    // SAFETY: getrusage writes into the zeroed struct we pass.
    let ru = unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        ru
    };
    let tv = |t: libc::timeval| Duration::new(t.tv_sec as u64, t.tv_usec as u32 * 1000);
    tv(ru.ru_utime) + tv(ru.ru_stime)
}

const BENCH_WARMUP: Duration = Duration::from_secs(3);

/// `--bench`: measures process CPU and per-frame main-thread time over a fixed window.
struct Bench {
    duration: Duration,
    first_frame: Option<Instant>,
    start: Option<(Instant, Duration)>,
    frames: u32,
    frame_work: Duration,
    net_to_screen: Duration,
}

impl Bench {
    fn new(duration: Duration) -> Self {
        Self {
            duration,
            first_frame: None,
            start: None,
            frames: 0,
            frame_work: Duration::ZERO,
            net_to_screen: Duration::ZERO,
        }
    }

    /// The window was hidden: decoding paused, so start over (with a fresh warm-up).
    fn restart(&mut self) {
        if self.start.is_some() {
            tracing::warn!("bench: window hidden, restarting measurement");
        }
        *self = Self::new(self.duration);
    }

    /// Records one presented frame. Returns the report when the window is over.
    fn frame(&mut self, now: Instant, work: Duration, net_to_screen: Duration) -> Option<String> {
        let first = *self.first_frame.get_or_insert(now);
        if now - first < BENCH_WARMUP {
            return None;
        }
        let (start, cpu0) = *self.start.get_or_insert_with(|| (now, process_cpu_time()));
        self.frames += 1;
        self.frame_work += work;
        self.net_to_screen += net_to_screen;
        let wall = now - start;
        if wall < self.duration {
            return None;
        }
        let cpu = process_cpu_time() - cpu0;
        let n = f64::from(self.frames.max(1));
        Some(format!(
            "frames={} fps={:.1} cpu={:.1}% of one core ({:.2} ms CPU/frame) \
             main-thread work/frame={:.3} ms net->screen={:.2} ms",
            self.frames,
            n / wall.as_secs_f64(),
            cpu.as_secs_f64() / wall.as_secs_f64() * 100.0,
            cpu.as_secs_f64() * 1000.0 / n,
            self.frame_work.as_secs_f64() * 1000.0 / n,
            self.net_to_screen.as_secs_f64() * 1000.0 / n,
        ))
    }
}

impl App {
    fn flush_input(&mut self) {
        for cmd in self.cmds.drain(..) {
            self.hid.send(cmd);
        }
    }

    fn set_visible(&mut self, visible: bool) {
        if !visible && let Some(b) = self.bench.as_mut() {
            b.restart();
        }
        self.pipeline.set_visible(visible);
    }

    fn draw(&mut self) {
        let Some(r) = self.renderer.as_mut() else {
            return;
        };
        if let Err(e) = r.render(self.mapper.rect()) {
            tracing::warn!(%e, "render failed");
        }
    }

    /// Returns a benchmark report when `--bench` has finished.
    fn on_frame(&mut self) -> Option<String> {
        let decoded = self.pipeline.take()?;
        let renderer = self.renderer.as_mut()?;
        let work_start = Instant::now();
        if !renderer.set_frame(&decoded.frame) {
            return None;
        }
        let video = (decoded.frame.width() as u32, decoded.frame.height() as u32);
        let surface = renderer.surface_size();
        self.mapper.set_layout(surface, video);
        self.draw();

        let now = Instant::now();
        let report = self
            .bench
            .as_mut()
            .and_then(|b| b.frame(now, now - work_start, now - decoded.received_at));
        self.stats.frames += 1;
        self.stats.net_to_screen += now - decoded.received_at;
        self.stats.decode += decoded.decoded_at - decoded.received_at;
        if now - self.stats.window_start >= Duration::from_secs(1) {
            self.update_title(video, now);
        }
        report
    }

    fn update_title(&mut self, video: (u32, u32), now: Instant) {
        let s = &self.stats;
        let secs = (now - s.window_start).as_secs_f64();
        let n = s.frames.max(1);
        let media = *self.media_stats.borrow();
        let title = format!(
            "{} — {}×{} · {:.0} fps · {:.1} Mb/s · net→screen {:.1} ms · decode {:.1} ms · dropped {}",
            self.title,
            video.0,
            video.1,
            f64::from(s.frames) / secs,
            media.kbps / 1000.0,
            s.net_to_screen.as_secs_f64() * 1000.0 / f64::from(n),
            s.decode.as_secs_f64() * 1000.0 / f64::from(n),
            media.dropped_total + self.pipeline.replaced(),
        );
        if let Some(w) = &self.window {
            w.set_title(&title);
        }
        tracing::debug!("{title}");
        self.stats = ViewStats::new();
    }

    fn status_title(&self) -> String {
        let conn = self.conn.borrow().clone();
        let media = self.media_state.borrow().clone();
        format!("{} — input: {conn:?} · video: {media:?}", self.title)
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(&self.title)
            .with_inner_size(LogicalSize::new(1280.0, 720.0));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                tracing::error!(%e, "cannot create window");
                event_loop.exit();
                return;
            }
        };
        self.scale_factor = window.scale_factor();
        match pollster::block_on(Renderer::new(window.clone(), self.render)) {
            Ok(r) => self.renderer = Some(r),
            Err(e) => {
                tracing::error!(%e, "cannot initialise renderer");
                event_loop.exit();
                return;
            }
        }
        window.set_title(&self.status_title());
        window.focus_window();
        self.window = Some(window);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Frame => {
                if let Some(report) = self.on_frame() {
                    let path = self.renderer.as_ref().map_or("?", Renderer::path_name);
                    println!("bench: path={path} {report}");
                    self.bench = None;
                    self.hid.release_all();
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.hid.release_all();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(r) = self.renderer.as_mut() {
                    r.resize(size.width, size.height);
                    let video = r.video_size().unwrap_or((16, 9));
                    self.mapper.set_layout((size.width, size.height), video);
                }
                self.draw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = scale_factor
            }
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::Occluded(occluded) => self.set_visible(!occluded),
            WindowEvent::Focused(false) => {
                self.mapper.focus_lost(&mut self.cmds);
                self.flush_input();
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                // winit synthesizes presses for keys already held when focus arrives.
                if is_synthetic {
                    return;
                }
                if let PhysicalKey::Code(code) = event.physical_key {
                    let pressed = event.state == ElementState::Pressed;
                    self.mapper.key(code, pressed, event.repeat, &mut self.cmds);
                    self.flush_input();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mapper.cursor((position.x, position.y), &mut self.cmds);
                self.flush_input();
            }
            WindowEvent::CursorLeft { .. } => self.mapper.cursor_left(),
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    winit::event::MouseButton::Left => MouseButton::Left,
                    winit::event::MouseButton::Right => MouseButton::Right,
                    winit::event::MouseButton::Middle => MouseButton::Middle,
                    winit::event::MouseButton::Back => MouseButton::Back,
                    winit::event::MouseButton::Forward => MouseButton::Forward,
                    winit::event::MouseButton::Other(_) => return,
                };
                self.mapper
                    .button(button, state == ElementState::Pressed, &mut self.cmds);
                self.flush_input();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        self.mapper
                            .wheel_lines(f64::from(x), f64::from(y), &mut self.cmds);
                    }
                    MouseScrollDelta::PixelDelta(p) => {
                        let s = self.scale_factor.max(0.1);
                        self.mapper.wheel_pixels(p.x / s, p.y / s, &mut self.cmds);
                    }
                }
                self.flush_input();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Until video flows, show connection state in the title (cheap; only on events).
        if self.stats.frames == 0
            && self
                .renderer
                .as_ref()
                .and_then(Renderer::video_size)
                .is_none()
            && let Some(w) = &self.window
        {
            w.set_title(&self.status_title());
        }
    }
}
