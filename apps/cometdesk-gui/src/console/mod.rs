//! A connected device: video, input, toolbar and status.
//!
//! Power rule (PLAN.md §3): nothing renders continuously. While the window is hidden,
//! decoding stops at once; after 30 s the video stream is dropped and the event socket
//! switches to `stream=0`, so the device can stop encoding. Showing the window resumes both.

mod audio;
mod capture;
mod clip;
mod files;
mod input;
mod menus;
mod palette;
mod session;
mod state;
mod stream;
mod target;
mod target_setup;
#[cfg(test)]
mod tests;
mod typing;
mod ui;
mod video;

use std::sync::Arc;
use std::time::{Duration, Instant};

use cometdesk::api::capabilities::Capabilities;
use cometdesk::api::system::{VideoFormat, VideoMode};
use cometdesk::events::{ConnState, DeviceState, EventOptions, EventSession, HidSender};
use cometdesk::hid::HidCommand;
use cometdesk::{Session, SystemConfig};
use cometdesk_clip::SessionState;
use cometdesk_media::Transport;
use cometdesk_view::{InputMapper, ScrollOptions};
use tokio::runtime::Handle;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use winit::event_loop::EventLoopProxy;

use self::state::{Confirm, FrameStats, RegionPick, Stream, Tab, Typing, VideoInfo};
use crate::actions::HostModifiers;
use crate::app::UserEvent;
use crate::clipsync::{ClipBridge, InsertResult};
use crate::media::Media;
use crate::setup::BridgeSetup;
use crate::store::{self, SavedDevice};

const HIDDEN_GRACE: Duration = Duration::from_secs(30);
const NOTICE_TIME: Duration = Duration::from_secs(4);
/// Pause after each key event we type ourselves, so the device's HID queue keeps up.
const KEY_GAP: Duration = Duration::from_millis(5);
/// Upper bound on waiting for the agent's screenshot reply, so a stalled bridge never hangs
/// the request forever.
const CAPTURE_REPLY_TIMEOUT: Duration = Duration::from_secs(15);

/// Width reserved for the status pill in the toolbar: fits the longest status text.
const STATUS_WIDTH: f32 = 132.0;

/// How long the pointer rests on the top edge before the fullscreen toolbar appears.
const EDGE_DWELL: std::time::Duration = std::time::Duration::from_millis(350);

/// Everything the connect task produces.
pub struct Connected {
    pub device: SavedDevice,
    pub session: Session,
    pub caps: Capabilities,
    pub config: SystemConfig,
    /// Keyboard layouts the device can type with.
    pub keymaps: Vec<String>,
}

/// What the UI asks the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleAction {
    None,
    Disconnect,
    ToggleFullscreen,
    TargetChanged,
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
    /// A menu or popup is open (keeps the fullscreen toolbar up; clicks go to egui).
    menu_open: bool,
    /// Toolbar rectangle in points while it is shown.
    toolbar_rect: Option<egui::Rect>,
    /// Since when the pointer rests on the top edge in fullscreen.
    edge_since: Option<Instant>,
    /// The open tab of the right-hand panel.
    drawer: Option<Tab>,
    /// The action search while open: its query.
    palette: Option<String>,
    typing: Option<Typing>,
    /// Outcome of an insert through the agent, while one is in flight.
    inserting: Option<tokio::sync::oneshot::Receiver<InsertResult>>,
    confirm: Option<Confirm>,
    window_focused: bool,
    /// A rectangle being dragged on the video, to copy that part of the screen.
    region: Option<RegionPick>,
    /// Where the video is drawn, in window points.
    video_pts: Option<egui::Rect>,
    /// Files on their way to the target through the agent.
    transfers: Vec<crate::clipsync::FileSend>,
    /// The target's file browser window.
    browser: crate::browser::Browser,
    /// A volume the user set a moment ago, shown until the target reports it back.
    volume_edit: Option<(u8, Instant)>,
    /// Files chosen in the "Send file" dialog, picked up by the next frame.
    picked_files: Arc<std::sync::Mutex<Vec<std::path::PathBuf>>>,
    notice: Option<(String, Instant)>,
    stats: FrameStats,
    hidden_since: Option<Instant>,
    /// Area for the video, in logical points (below the toolbar).
    video_area: egui::Rect,
    /// Clipboard bridge to the agent on the target; `None` until paired.
    clip: Option<ClipBridge>,
    clip_paused: bool,
    /// Pairing code shown in the target setup dialog while it is open.
    target_setup: Option<String>,
    /// Comet-side health and setup of the bridge.
    setup: BridgeSetup,
    /// A "needs repair" notice was shown for this connection.
    repair_noticed: bool,
    /// The target OS changed outside the toolbar (dialog, agent report): save the device.
    target_changed: bool,
    transport: Transport,
    audio_on: bool,
    video_info: VideoInfo,
    video_info_rx: Option<tokio::sync::oneshot::Receiver<VideoInfo>>,
    /// Frame rate and bitrate (kbit/s) edited in the video menu, not applied yet.
    encoder_draft: Option<(i64, i64, i64)>,
    /// Output device names, cached while the audio-output submenu is open (enumerating them
    /// every frame is slow); with the time they were read, for a short TTL.
    audio_outputs: Option<(Vec<String>, Instant)>,
    media: Media,
    /// Last keyboard layout from the agent we applied; a new report is applied again.
    adopted_keyboard: Option<String>,
    /// Last uptime the agent reported and when, to show a running value between reports.
    uptime_seen: Option<(u64, Instant)>,
    /// What the agent last said about the session, to announce changes.
    last_session: Option<SessionState>,
    agent_was_up: bool,
    clip_expanded: bool,
    info_expanded: bool,
    media_expanded: bool,
}

impl Console {
    pub fn new(c: Connected, rt: Handle, proxy: EventLoopProxy<UserEvent>, iso_fix: bool) -> Self {
        let scroll =
            ScrollOptions::from_config(c.config.scroll_rate, c.config.reverse_scrolling.as_deref());
        let mapper = InputMapper::new(c.device.target.key_options(iso_fix), scroll);
        let _guard = rt.enter();
        let events = EventSession::spawn(c.session.clone(), EventOptions::default());
        let setup = {
            let proxy = proxy.clone();
            BridgeSetup::new(rt.clone(), c.session.clone(), move || {
                let _ = proxy.send_event(UserEvent::Refresh);
            })
        };
        let media = Media::new(c.session.clone(), rt.clone(), proxy.clone());
        let mode = c.config.video_mode.as_deref().and_then(VideoMode::parse);
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
            menu_open: false,
            toolbar_rect: None,
            edge_since: None,
            drawer: debug_view()
                .filter(|v| v.starts_with("panel"))
                .map(|v| match v.as_str() {
                    "panel-video" => Tab::Video,
                    "panel-settings" => Tab::Settings,
                    _ => Tab::Target,
                }),
            palette: debug_view()
                .filter(|v| v == "palette")
                .map(|_| String::new()),
            typing: None,
            inserting: None,
            confirm: None,
            window_focused: true,
            region: None,
            video_pts: None,
            transfers: Vec::new(),
            browser: crate::browser::Browser::default(),
            volume_edit: None,
            picked_files: Arc::default(),
            notice: None,
            stats: FrameStats::default(),
            hidden_since: None,
            video_area: egui::Rect::NOTHING,
            clip: None,
            clip_paused: false,
            target_setup: None,
            setup,
            repair_noticed: false,
            target_changed: false,
            transport: video::transport_for_mode(mode),
            audio_on: false,
            video_info: VideoInfo {
                codec: c.config.video_format.and_then(VideoFormat::from_wire),
                mode,
                ..VideoInfo::default()
            },
            video_info_rx: None,
            encoder_draft: None,
            audio_outputs: None,
            media,
            adopted_keyboard: None,
            uptime_seen: None,
            last_session: None,
            agent_was_up: false,
            clip_expanded: true,
            info_expanded: false,
            media_expanded: false,
        };
        console.audio_on = console.device.audio;
        console.refresh_video_info();
        console.media.refresh();
        console.start_stream();
        // The keychain read can block on a macOS ACL prompt: load the pairing secret off the
        // UI thread and start the bridge when it arrives (see `on_clip_secret`).
        let device = console.device.clone();
        let proxy = console.proxy.clone();
        console.rt.spawn_blocking(move || {
            if let Some(secret) = store::clip_secret(&device) {
                let _ = proxy.send_event(UserEvent::ClipSecret(secret));
            }
        });
        console
    }
}

/// Debug builds: `COMETDESK_DEBUG_UI` opens a view at start so screenshots need no clicks.
#[cfg(debug_assertions)]
fn debug_view() -> Option<String> {
    std::env::var("COMETDESK_DEBUG_UI")
        .ok()
        .filter(|v| !v.is_empty())
}

#[cfg(not(debug_assertions))]
fn debug_view() -> Option<String> {
    None
}
