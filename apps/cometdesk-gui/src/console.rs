//! A connected device: video, input, toolbar and status.
//!
//! Power rule (PLAN.md §3): nothing renders continuously. While the window is hidden,
//! decoding stops at once; after 30 s the video stream is dropped and the event socket
//! switches to `stream=0`, so the device can stop encoding. Showing the window resumes both.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use cometdesk::api::atx::{AtxButton, PowerAction};
use cometdesk::api::bridge::Health;
use cometdesk::api::capabilities::{Capabilities, Feature};
use cometdesk::api::edid::{self, EdidPreset};
use cometdesk::api::hid::PrintOptions;
use cometdesk::api::streamer::{
    QualityPreset, SnapshotOptions, StreamerLimits, StreamerParams, StreamerUpdate,
};
use cometdesk::api::system::{LatencyMode, Orientation, VencMode, VideoFormat, VideoMode};
use cometdesk::events::{ConnState, DeviceState, EventOptions, EventSession, HidSender};
use cometdesk::hid::{HidCommand, MouseButton};
use cometdesk::{Session, SystemConfig};
use cometdesk_clip::{Hello, Info, PastePref, PowerAction as SessionAction, SessionState};
use cometdesk_media::audio::{AudioSession, Player};
use cometdesk_media::direct::{MediaSession, SourceState};
use cometdesk_media::{SourceOptions, Transport};
use cometdesk_view::pipeline::FramePipeline;
use cometdesk_view::{InputMapper, Rect, ScrollOptions, letterbox_in};
use tokio::runtime::Handle;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use winit::event_loop::EventLoopProxy;
use winit::keyboard::KeyCode;

use crate::actions::{self, HostModifiers, Shortcut};
use crate::app::UserEvent;
use crate::clipsync::{
    BridgeState, ClipBridge, InsertResult, Note, Request, Settings as ClipSettings,
};
use crate::gpu::Gpu;
use crate::media::Media;
use crate::setup::BridgeSetup;
use crate::store::{self, ClipDirection, SavedDevice, TargetOs};
use crate::theme;

const HIDDEN_GRACE: Duration = Duration::from_secs(30);
const NOTICE_TIME: Duration = Duration::from_secs(4);
/// Pause after each key event we type ourselves, so the device's HID queue keeps up.
const KEY_GAP: Duration = Duration::from_millis(5);
/// Upper bound on waiting for the agent's screenshot reply, so a stalled bridge never hangs
/// the request forever.
const CAPTURE_REPLY_TIMEOUT: Duration = Duration::from_secs(15);

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
    audio: Option<Audio>,
}

/// Target audio: its own WebRTC connection plus the output stream. Lives and dies with the
/// video stream, so a hidden window plays and fetches nothing.
struct Audio {
    _player: Player,
    _session: AudioSession,
}

struct Typing {
    total: usize,
    done: Arc<AtomicUsize>,
    task: JoinHandle<Result<(), String>>,
}

/// A region being picked: the drag's start (once it began) and the pointer now.
struct RegionPick {
    start: Option<egui::Pos2>,
    now: egui::Pos2,
}

enum Confirm {
    Type {
        text: String,
        reason: String,
    },
    Power(PowerAction),
    Click(AtxButton),
    /// An action the agent carries out on the target's OS (log off, restart, shut down).
    Session(SessionAction),
    /// Install / update / repair the bridge on the Comet; holds the action's name.
    BridgeSetup(String),
    Video(VideoChange),
}

/// A device-wide video setting the user asked to change.
#[derive(Clone)]
enum VideoChange {
    Mode(VideoMode),
    Encoder(StreamerUpdate),
    Codec(VideoFormat),
    Venc(VencMode),
    Latency(LatencyMode),
    Orientation(Orientation),
    Edid(EdidPreset),
}

/// Device-wide video settings as last read from the device.
#[derive(Default)]
struct VideoInfo {
    codec: Option<VideoFormat>,
    h265_supported: bool,
    mode: Option<VideoMode>,
    venc: Option<VencMode>,
    latency: Option<LatencyMode>,
    orientation: Orientation,
    /// Normalized hex of the EDID the target currently sees.
    edid: String,
    edid_presets: Vec<EdidPreset>,
    params: StreamerParams,
    limits: StreamerLimits,
    source_fps: Option<u32>,
}

/// Reads the device-wide video settings.
async fn read_video_info(session: &Session) -> VideoInfo {
    let (cfg, st, edid, presets) = tokio::join!(
        session.system_config(),
        session.streamer_state(),
        session.edid(),
        session.edid_presets(),
    );
    let cfg = cfg.ok().unwrap_or_default();
    let st = st.unwrap_or_default();
    let source_fps = st.source_fps();
    let codec = cfg.video_format.and_then(VideoFormat::from_wire);
    let h265_supported = st.features.h265;
    VideoInfo {
        codec,
        h265_supported,
        mode: cfg.video_mode.as_deref().and_then(VideoMode::parse),
        venc: cfg.venc_mode.as_deref().and_then(VencMode::parse),
        latency: cfg.video_processing.as_deref().and_then(LatencyMode::parse),
        orientation: cfg
            .orientation
            .and_then(Orientation::from_degrees)
            .unwrap_or_default(),
        edid: edid.unwrap_or_default(),
        edid_presets: presets.unwrap_or_default(),
        params: st.params,
        limits: st.limits,
        source_fps,
    }
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
            drawer: None,
            palette: None,
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
            transport: Transport::default(),
            audio_on: false,
            video_info: VideoInfo {
                codec: c.config.video_format.and_then(VideoFormat::from_wire),
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
        };
        console.transport = std::env::var("COMETDESK_TRANSPORT")
            .ok()
            .and_then(|t| t.parse().ok())
            .or_else(|| console.device.transport.parse().ok())
            .unwrap_or_default();
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

    /// The stored pairing secret finished loading: start the clipboard bridge with it.
    pub fn on_clip_secret(&mut self, secret: cometdesk_clip::Secret) {
        if self.clip.is_none() {
            self.start_clip(secret);
        }
    }

    // ----- clipboard bridge ----------------------------------------------------------------

    fn clip_settings(&self) -> ClipSettings {
        ClipSettings {
            direction: self.device.clip_direction,
            paused: self.clip_paused,
        }
    }

    fn start_clip(&mut self, secret: cometdesk_clip::Secret) {
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

    /// New pairing code (replacing any old pairing) and the dialog that installs the agent
    /// on the target with it.
    fn start_target_setup(&mut self) {
        let secret = cometdesk_clip::Secret::generate();
        // The keychain write can block on a macOS ACL prompt: store the new code off the UI
        // thread. The bridge uses the in-memory copy, so it starts regardless.
        let (device, stored) = (self.device.clone(), secret.clone());
        self.rt.spawn_blocking(move || {
            if let Err(e) = store::set_clip_secret(&device, &stored) {
                tracing::warn!(%e, "could not store the pairing code");
            }
        });
        self.target_setup = Some(secret.to_code());
        self.start_clip(secret);
    }

    /// Changes the target OS (modifier mapping, install command) and saves it.
    fn set_target(&mut self, os: TargetOs) {
        if self.device.target == os {
            return;
        }
        self.device.target = os;
        let iso = self.mapper.keys.iso_fix;
        self.mapper.keys = os.key_options(iso);
        self.target_changed = true;
    }

    /// The agent reports the target's real OS: correct a wrong (e.g. default) setting.
    fn adopt_agent_os(&mut self) {
        let Some((hello, info)) = self.agent() else {
            if std::mem::take(&mut self.agent_was_up) {
                self.last_session = None;
                self.notify("Agent disconnected: the target slept, restarted or logged out".into());
            }
            return;
        };
        self.agent_was_up = true;
        self.track_agent_info(&info);
        self.adopt_agent_keymap(info.keyboard.as_deref());
        let reported = match hello.os.as_str() {
            "linux" => TargetOs::Linux,
            "windows" => TargetOs::Windows,
            "macos" => TargetOs::MacOs,
            _ => return,
        };
        if self.device.target != reported && self.device.target != TargetOs::Bios {
            self.set_target(reported);
            self.notify(format!(
                "Target set to {} (reported by the agent)",
                reported.label()
            ));
        }
    }

    /// The agent's hello and latest report, while it is connected.
    fn agent(&self) -> Option<(Hello, Info)> {
        match self.clip.as_ref()?.state() {
            BridgeState::Connected(h, i) => Some((h, i)),
            _ => None,
        }
    }

    fn track_agent_info(&mut self, info: &Info) {
        if let Some(up) = info.uptime_secs
            && self.uptime_seen.is_none_or(|(u, _)| u != up)
        {
            self.uptime_seen = Some((up, Instant::now()));
        }
        let before = self.last_session.replace(info.session);
        match (before, info.session) {
            (_, SessionState::Locked) if before != Some(SessionState::Locked) => {
                self.notify("Target is locked: typed text goes to the lock screen".into());
            }
            (Some(SessionState::Locked), SessionState::Active) => {
                self.notify("Target unlocked".into());
            }
            _ => {}
        }
    }

    /// The agent reports the target's active keyboard layout: use it for typing, once per
    /// change, so a manual choice sticks until the target's layout changes.
    fn adopt_agent_keymap(&mut self, reported: Option<&str>) {
        let Some(reported) = reported else { return };
        if self.adopted_keyboard.as_deref() == Some(reported) {
            return;
        }
        self.adopted_keyboard = Some(reported.to_owned());
        let Some(map) = cometdesk::layout::keymap_from_native(reported) else {
            return;
        };
        if self.keymap().as_deref() != Some(map) && self.keymaps.iter().any(|k| k == map) {
            self.device.keymap = Some(map.to_owned());
            self.target_changed = true;
            self.notify(format!("Layout set to {map} (reported by the agent)"));
        }
    }

    fn forget_clipboard_pairing(&mut self) {
        let device = self.device.clone();
        self.rt
            .spawn_blocking(move || store::delete_clip_secret(&device));
        self.clip = None;
        self.notify("Clipboard pairing removed".into());
    }

    /// What the Comet side needs, as a button label (`None`: nothing to do).
    fn bridge_action(&self) -> Option<&'static str> {
        match self.setup.health()? {
            Health::NotInstalled => Some("Set up this Comet…"),
            Health::Outdated { .. } => Some("Update the Comet's bridge…"),
            Health::Broken(_) => Some("Repair the Comet's bridge…"),
            Health::Ok => None,
        }
    }

    fn clip_menu(&mut self, ui: &mut egui::Ui) -> ConsoleAction {
        let mut action = ConsoleAction::None;
        let needs_device = self.bridge_action().is_some();
        // The label takes the sync state's color: green synced, amber needs attention.
        let (state_text, tint) = match self.clip.as_ref().map(ClipBridge::state) {
            _ if self.setup.progress().is_some() => ("setting up…", Some(theme::WARN)),
            Some(_) if needs_device => ("needs repair", Some(theme::WARN)),
            None => ("off", None),
            Some(_) if self.clip_paused => ("paused", None),
            Some(BridgeState::Connected(..)) => ("synced", Some(theme::OK)),
            Some(_) => ("not connected", Some(theme::WARN)),
        };
        let mut title = egui::RichText::new("Clipboard");
        if let Some(c) = tint {
            title = title.color(c);
        }
        theme::section(ui, title, |ui| {
            ui.set_min_width(260.0);
            ui.label(egui::RichText::new(format!("Clipboard: {state_text}")).strong());
            // The Comet side.
            if let Some(p) = self.setup.progress() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(p);
                });
            } else {
                match self.setup.health() {
                    None => {
                        ui.label("Checking the Comet…");
                    }
                    Some(Health::Ok) => {
                        let fw = self
                            .setup
                            .status()
                            .and_then(|s| s.firmware)
                            .unwrap_or_default();
                        ui.label(format!("Comet: bridge ready {fw}"));
                    }
                    Some(h) => {
                        ui.colored_label(theme::WARN, health_text(&h));
                    }
                }
                if let Some(label) = self.bridge_action()
                    && ui.button(label).clicked()
                {
                    self.confirm = Some(Confirm::BridgeSetup(label.trim_end_matches('…').into()));
                    ui.close();
                }
            }
            ui.separator();

            // The target side.
            let Some(state) = self.clip.as_ref().map(ClipBridge::state) else {
                ui.label("Shares the clipboard with the target.");
                if ui
                    .add_enabled(!needs_device, egui::Button::new("Set up the target…"))
                    .clicked()
                {
                    self.start_target_setup();
                    ui.close();
                }
                return;
            };
            ui.label(format!("Target: {}", state.label()));
            let before = self.device.clip_direction;
            for d in ClipDirection::ALL {
                ui.radio_value(&mut self.device.clip_direction, d, d.label());
            }
            if self.device.clip_direction != before {
                action = ConsoleAction::TargetChanged;
            }
            ui.checkbox(&mut self.clip_paused, "Pause");
            if self.device.target == TargetOs::Linux {
                ui.separator();
                ui.label("Insert clipboard pastes with:");
                let before = self.device.insert_paste;
                for (p, l) in [
                    (PastePref::Auto, "Auto (Ctrl+Shift+V in terminals)"),
                    (PastePref::CtrlV, "Ctrl+V"),
                    (PastePref::CtrlShiftV, "Ctrl+Shift+V"),
                ] {
                    ui.radio_value(&mut self.device.insert_paste, p, l);
                }
                if self.device.insert_paste != before {
                    action = ConsoleAction::TargetChanged;
                }
            }
            if let Some(c) = &self.clip
                && c.settings() != self.clip_settings()
            {
                c.set_settings(self.clip_settings());
            }
            ui.separator();
            if ui
                .button("Set up the target again…")
                .on_hover_text("Reinstalls the agent on the target with a new pairing code")
                .clicked()
            {
                self.start_target_setup();
                ui.close();
            }
            if ui.button("Forget pairing").clicked() {
                self.forget_clipboard_pairing();
                ui.close();
            }
        });

        action
    }

    /// Installs, pairs and starts the agent on the target: one command, typed for the user.
    fn target_setup_dialog(&mut self, ctx: &egui::Context) {
        let Some(code) = self.target_setup.clone() else {
            return;
        };
        let os = self.device.target;
        let mut chosen = os;
        let command = install_command(os, &code);
        let agent = agent_name(os);
        let missing_agent = agent.is_some_and(|a| !self.setup.has_agent(a));
        let state = self.clip.as_ref().map(ClipBridge::state);
        let connected = matches!(state, Some(BridgeState::Connected(..)));
        let mut close = false;
        egui::Modal::new(egui::Id::unique("target-setup")).show(ctx, |ui| {
            ui.set_max_width(560.0);
            ui.label(egui::RichText::new("Set up the target").size(17.0).strong());
            ui.horizontal(|ui| {
                ui.label("The target runs:");
                for os in [TargetOs::Linux, TargetOs::Windows, TargetOs::MacOs] {
                    ui.selectable_value(&mut chosen, os, os.label());
                }
            });
            if connected {
                ui.colored_label(
                    theme::OK,
                    "The agent is connected: the clipboard is shared now.",
                );
            }
            let Some(command) = &command else {
                ui.label(format!(
                    "The clipboard agent does not run on {} targets yet. \
                     Pick the target's OS in the toolbar if it is wrong.",
                    os.label()
                ));
                ui.label("Manual pairing on a supported system:");
                ui.monospace(format!("cometdesk-agent pair {code}"));
                if ui.button("Close").clicked() {
                    close = true;
                }
                return;
            };
            if missing_agent {
                ui.colored_label(
                    theme::WARN,
                    "The Comet has no agent program for this system yet: build it with \
                     scripts/build-agents.sh, then update the Comet's bridge.",
                );
            }
            ui.label(format!(
                "1. Open a {} on the target and click into it.",
                if os == TargetOs::Windows {
                    "Run dialog or PowerShell"
                } else {
                    "terminal"
                }
            ));
            let (open_label, open_keys): (&str, &[&'static str]) = match os {
                TargetOs::Windows => ("Open the Run dialog (Win+R)", &["MetaLeft", "KeyR"]),
                _ => (
                    "Open a terminal (Ctrl+Alt+T)",
                    &["ControlLeft", "AltLeft", "KeyT"],
                ),
            };
            if ui.button(open_label).clicked() {
                self.send_combo(open_keys);
            }
            ui.label(
                "2. Run this command there. It downloads the agent from the Comet over the \
                      USB cable, pairs it and starts it (also at every login). No admin rights \
                      or internet needed.",
            );
            ui.add(
                egui::TextEdit::multiline(&mut command.as_str())
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                if ui
                    .button("Run it on the target")
                    .on_hover_text("Types the command and presses Enter in the focused window")
                    .clicked()
                {
                    self.start_typing(format!("{command}\n"));
                }
                if ui.button("Copy").clicked() {
                    ui.ctx().copy_text(command.clone());
                }
            });
            if let Some(s) = &state
                && !connected
            {
                ui.label(format!("Status: {}", s.label()));
            }
            if ui
                .button(if connected { "Done" } else { "Close" })
                .clicked()
            {
                close = true;
            }
        });
        if chosen != os {
            self.set_target(chosen);
        }
        if close {
            self.target_setup = None;
        }
    }

    fn start_stream(&mut self) {
        let _guard = self.rt.enter();
        let prefer = self
            .video_info
            .codec
            .map(Into::into)
            .unwrap_or(cometdesk_media::Codec::H264);
        if self.transport.for_codec(prefer) != self.transport {
            self.notify(
                "WebRTC with error correction can't carry H.265; using Direct while the device is on H.265"
                    .into(),
            );
        }
        let media = cometdesk_media::start(
            self.session.clone(),
            SourceOptions {
                transport: self.transport,
                prefer,
            },
        );
        let (video, media) = media.take_video();
        let proxy = self.proxy.clone();
        match FramePipeline::spawn(video, media.control.clone(), move || {
            let _ = proxy.send_event(UserEvent::Frame);
        }) {
            Ok(pipeline) => {
                let audio = self.start_audio();
                self.stream = Some(Stream {
                    media,
                    pipeline,
                    audio,
                });
            }
            Err(e) => self.notify(format!("cannot start decoder: {e}")),
        }
        self.restart_watcher();
    }

    /// Audio comes from the source matching the device's video mode (Janus in `webrtc`, Pion
    /// in `flex_fec`), independent of how video reaches us, and only while it is switched on.
    fn start_audio(&mut self) -> Option<Audio> {
        if !self.audio_on {
            return None;
        }
        let _guard = self.rt.enter();
        // Unknown until the first read of the device state; see where `video_info` arrives.
        let mode = self.video_info.mode?.wire_name();
        let Some(source) = cometdesk_media::audio::start(self.session.clone(), mode) else {
            self.audio_on = false;
            self.notify(
                "Audio needs the device in webrtc, flex_fec or direct_h264 video mode".into(),
            );
            return None;
        };
        let (packets, session) = source.take_packets();
        let output = Some(self.device.audio_output.as_str()).filter(|n| !n.is_empty());
        match Player::start(packets, output) {
            Ok(player) => Some(Audio {
                _player: player,
                _session: session,
            }),
            Err(e) => {
                self.audio_on = false;
                self.notify(format!("Audio unavailable: {e}"));
                None
            }
        }
    }

    fn set_audio(&mut self, on: bool) {
        if self.device.audio != on {
            self.device.audio = on;
            self.target_changed = true;
        }
        self.audio_on = on;
        let audio = if on { self.start_audio() } else { None };
        if let Some(s) = &mut self.stream {
            s.audio = audio;
        }
    }

    /// Audio output device names, re-enumerated at most once per [`AUDIO_OUTPUTS_TTL`] so the
    /// submenu doesn't query the OS every frame it is open.
    fn audio_outputs(&mut self) -> Vec<String> {
        const AUDIO_OUTPUTS_TTL: Duration = Duration::from_secs(2);
        let fresh = self
            .audio_outputs
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() < AUDIO_OUTPUTS_TTL);
        if !fresh {
            self.audio_outputs = Some((cometdesk_media::audio::output_devices(), Instant::now()));
        }
        self.audio_outputs.as_ref().unwrap().0.clone()
    }

    fn set_transport(&mut self, transport: Transport) {
        if transport == self.transport {
            return;
        }
        self.transport = transport;
        self.device.transport = transport.name().into();
        self.target_changed = true;
        self.stream = None;
        self.start_stream();
        self.notify(format!("Video source: {}", transport.name()));
    }

    /// Reads the device-wide video mode and encoder settings in the background.
    fn refresh_video_info(&mut self) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.video_info_rx = Some(rx);
        let session = self.session.clone();
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            let _ = tx.send(read_video_info(&session).await);
            let _ = proxy.send_event(UserEvent::Refresh);
        });
    }

    /// Writes a device-wide video setting, then re-reads the state. The read must follow the
    /// write inside one task: as separate tasks they race and the menu shows the old value.
    fn apply_video(&mut self, change: VideoChange) {
        let session = self.session.clone();
        let proxy = self.proxy.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.video_info_rx = Some(rx);
        self.rt.spawn(async move {
            let msg = match change {
                VideoChange::Mode(m) => session
                    .set_video_mode(m)
                    .await
                    .map(|()| format!("Video mode: {}", m.wire_name()))
                    .map_err(|e| format!("Cannot set video mode: {e}")),
                VideoChange::Encoder(u) => session
                    .set_streamer_params(u)
                    .await
                    .map(|_| "Encoder quality changed".to_string())
                    .map_err(|e| format!("Cannot set quality: {e}")),
                VideoChange::Codec(f) => session
                    .set_video_format(f)
                    .await
                    .map(|()| format!("Codec: {}", f.label()))
                    .map_err(|e| format!("Cannot set codec: {e}")),
                VideoChange::Venc(m) => session
                    .set_venc_mode(m)
                    .await
                    .map(|()| format!("Encoder mode: {}", m.wire_name()))
                    .map_err(|e| format!("Cannot set encoder mode: {e}")),
                VideoChange::Latency(m) => session
                    .set_latency_mode(m)
                    .await
                    .map(|()| "Latency mode changed".to_string())
                    .map_err(|e| format!("Cannot set latency mode: {e}")),
                VideoChange::Orientation(o) => session
                    .set_orientation(o)
                    .await
                    .map(|()| format!("Orientation: {}°", o.degrees()))
                    .map_err(|e| format!("Cannot set orientation: {e}")),
                VideoChange::Edid(p) => session
                    .set_edid(&p.content)
                    .await
                    .map(|()| format!("Display: {}", p.label))
                    .map_err(|e| format!("Cannot set the display: {e}")),
            };
            let _ = tx.send(read_video_info(&session).await);
            let _ = proxy.send_event(UserEvent::Notice(msg.unwrap_or_else(|e| e)));
        });
    }

    fn video_menu(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("This viewer").strong());
        ui.label(
            egui::RichText::new("Video source")
                .color(theme::MUTED)
                .small(),
        );
        for t in Transport::ALL {
            let label = match t {
                Transport::Direct => "Direct (LAN, lowest latency)",
                Transport::Pion => "Remote (WebRTC with error correction)",
                Transport::Janus => "Remote (WebRTC, Janus)",
            };
            if ui.radio(self.transport == t, label).clicked() {
                self.set_transport(t);
                ui.close();
            }
        }
        let mut audio = self.audio_on;
        let enabled = matches!(
            self.video_info.mode,
            Some(VideoMode::Webrtc | VideoMode::FlexFec | VideoMode::DirectH264)
        );
        if ui
            .add_enabled(enabled, egui::Checkbox::new(&mut audio, "Play audio"))
            .on_disabled_hover_text(
                "Audio needs the device in webrtc, flex_fec or direct_h264 video mode",
            )
            .changed()
        {
            self.set_audio(audio);
        }
        if self.audio_on {
            let devices = self.audio_outputs();
            let mut chosen = None;
            theme::section(ui, "Audio output", |ui| {
                let current = &self.device.audio_output;
                if ui.radio(current.is_empty(), "System default").clicked() {
                    chosen = Some(String::new());
                }
                for name in &devices {
                    if ui.radio(current == name, name).clicked() {
                        chosen = Some(name.clone());
                    }
                }
            });
            if let Some(name) = chosen {
                self.device.audio_output = name;
                self.target_changed = true;
                self.set_audio(true);
                ui.close();
            }
        }
        ui.separator();
        ui.label(
            egui::RichText::new("Whole device")
                .strong()
                .color(theme::WARN),
        );
        ui.label(
            egui::RichText::new("Changes apply to every viewer, after confirming")
                .color(theme::MUTED)
                .small(),
        );
        let info = &self.video_info;
        let mut change = None;
        theme::section(ui, "Streaming protocol", |ui| {
            for m in VideoMode::ALL {
                let on = info.mode == Some(m);
                if ui
                    .radio(on, video_mode_label(m))
                    .on_hover_text(m.wire_name())
                    .clicked()
                    && !on
                {
                    change = Some(VideoChange::Mode(m));
                }
            }
        });
        theme::section(ui, "Codec", |ui| {
            for format in VideoFormat::ALL {
                let on = info.codec == Some(format);
                let supported = match format {
                    VideoFormat::H264 => true,
                    VideoFormat::H265 => info.h265_supported,
                };
                let label = format.label();
                let radio = egui::RadioButton::new(on, label);
                if ui
                    .add_enabled(supported, radio)
                    .on_disabled_hover_text("H.265 is not supported by this device")
                    .clicked()
                    && !on
                {
                    change = Some(VideoChange::Codec(format));
                }
            }
        });
        theme::section(ui, "Bandwidth saving", |ui| {
            for (m, label) in [
                (VencMode::Smart, "Smart (less bandwidth)"),
                (VencMode::Normal, "Normal"),
            ] {
                let on = info.venc == Some(m);
                if ui
                    .radio(on, label)
                    .on_hover_text("Encoder mode: Smart reduces bandwidth")
                    .clicked()
                    && !on
                {
                    change = Some(VideoChange::Venc(m));
                }
            }
        });
        theme::section(ui, "Latency mode", |ui| {
            for (m, label) in [
                (LatencyMode::LowestLatency, "Lowest latency"),
                (LatencyMode::SmoothDisplay, "Smooth display"),
            ] {
                let on = info.latency == Some(m);
                if ui.radio(on, label).clicked() && !on {
                    change = Some(VideoChange::Latency(m));
                }
            }
        });
        theme::section(ui, "Orientation", |ui| {
            for o in Orientation::ALL {
                let on = info.orientation == o;
                if ui.radio(on, format!("{}°", o.degrees())).clicked() && !on {
                    change = Some(VideoChange::Orientation(o));
                }
            }
        });
        ui.add_enabled_ui(!info.edid_presets.is_empty(), |ui| {
            theme::section(ui, "Display resolution", |ui| {
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        let current = info
                            .edid_presets
                            .iter()
                            .find(|p| edid::normalize(&p.content) == info.edid);
                        if current.is_none() {
                            let _ = ui.radio(true, "Custom");
                        }
                        for p in &info.edid_presets {
                            let on = current.is_some_and(|c| c.key == p.key);
                            let label = if p.audio {
                                p.label.clone()
                            } else {
                                format!("{} (no audio)", p.label)
                            };
                            if ui.radio(on, label).clicked() && !on {
                                change = Some(VideoChange::Edid(p.clone()));
                            }
                        }
                    });
            });
        });
        ui.separator();
        self.encoder_controls(ui, &mut change);
        if let Some(c) = change {
            self.confirm = Some(Confirm::Video(c));
            ui.close();
        }
    }

    /// Frame rate and bitrate, each set on its own. Edits are kept as a draft until applied.
    fn encoder_controls(&mut self, ui: &mut egui::Ui, change: &mut Option<VideoChange>) {
        let info = &self.video_info;
        let p = &info.params;
        let (cur_fps, cur_kbps, cur_gop) = (p.desired_fps, p.h264_bitrate, p.h264_gop);
        let (f, b, g) = (
            info.limits.desired_fps,
            info.limits.h264_bitrate,
            info.limits.h264_gop,
        );
        let (fps_min, fps_max) = (f.min.max(1), f.max.max(f.min.max(1)));
        let (kbps_min, kbps_max) = (b.min, b.max.max(b.min));
        let (gop_min, gop_max) = (g.min.max(1), g.max.max(g.min.max(1)));
        let source = info.source_fps.map_or(60, i64::from);
        let (mut fps, mut kbps, mut gop) = self.encoder_draft.unwrap_or((
            cur_fps.unwrap_or(30),
            cur_kbps.unwrap_or(kbps_min),
            cur_gop.unwrap_or(gop_min),
        ));
        let shown = |v: Option<i64>| v.map_or("?".into(), |v| v.to_string());
        ui.label(format!(
            "Encoder: {} fps, {} kbit/s, keyframe every {} frames",
            shown(cur_fps),
            shown(cur_kbps),
            shown(cur_gop),
        ));
        ui.horizontal(|ui| {
            ui.label("Frame rate");
            ui.add(
                egui::DragValue::new(&mut fps)
                    .range(fps_min..=fps_max)
                    .suffix(" fps"),
            );
            for v in [30, source] {
                if (fps_min..=fps_max).contains(&v) && ui.small_button(v.to_string()).clicked() {
                    fps = v;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Bitrate");
            ui.add(
                egui::DragValue::new(&mut kbps)
                    .range(kbps_min..=kbps_max)
                    .speed(50)
                    .suffix(" kbit/s"),
            );
        });
        ui.horizontal(|ui| {
            for p in QualityPreset::ALL {
                let v = p.bitrate(&info.limits);
                if ui
                    .small_button(format!("{p:?}"))
                    .on_hover_text(format!("{v} kbit/s"))
                    .clicked()
                {
                    kbps = v;
                }
            }
        });
        // H.265 on this encoder sends every frame of a still screen and a full keyframe
        // each interval, so the interval sets its idle bitrate (~200 KB per keyframe at
        // 1440p). Our viewers ask for a keyframe when they need one.
        ui.horizontal(|ui| {
            ui.label("Keyframe every");
            ui.add(
                egui::DragValue::new(&mut gop)
                    .range(gop_min..=gop_max)
                    .suffix(" frames"),
            )
            .on_hover_text(format!("{:.1} s at {fps} fps", gop as f64 / fps as f64));
            for v in [fps, gop_max] {
                if (gop_min..=gop_max).contains(&v) && ui.small_button(v.to_string()).clicked() {
                    gop = v;
                }
            }
        });
        let update = StreamerUpdate {
            desired_fps: (Some(fps) != cur_fps).then_some(fps),
            h264_bitrate: (Some(kbps) != cur_kbps).then_some(kbps),
            h264_gop: (Some(gop) != cur_gop).then_some(gop),
            ..Default::default()
        };
        let edited = update != StreamerUpdate::default();
        self.encoder_draft = edited.then_some((fps, kbps, gop));
        if ui
            .add_enabled(edited, egui::Button::new("Apply…"))
            .clicked()
        {
            self.encoder_draft = None;
            *change = Some(VideoChange::Encoder(update));
        }
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
            let codec_prefix = media
                .codec
                .map_or(String::new(), |c| format!("{} · ", c.label()));
            let frame_sizes = if media.avg_keyframe_bytes > 0 || media.avg_delta_bytes > 0 {
                format!(
                    " · key {} B · delta {} B",
                    media.avg_keyframe_bytes, media.avg_delta_bytes
                )
            } else {
                String::new()
            };
            s.shown = format!(
                "{codec_prefix}{} fps · {:.1} Mb/s{frame_sizes} · decode {:.1} ms · net→screen {:.1} ms · dropped {} · {}",
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

    /// Clockwise degrees the renderer must rotate frames: only Direct frames arrive
    /// unrotated, the device rotates WebRTC video itself.
    pub fn video_rotation(&self) -> i32 {
        if self.transport == Transport::Direct {
            self.video_info.orientation.degrees()
        } else {
            0
        }
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
            let ppp = f64::from(pixels_per_point);
            self.video_pts = Some(egui::Rect::from_min_size(
                egui::pos2((rect.x / ppp) as f32, (rect.y / ppp) as f32),
                egui::vec2((rect.w / ppp) as f32, (rect.h / ppp) as f32),
            ));
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
        let locked = self.last_session == Some(SessionState::Locked);
        let reason = actions::confirm_reason(&text).or_else(|| {
            locked.then(|| "been aimed at a locked target: it goes to the lock screen".to_string())
        });
        match reason {
            Some(reason) => self.confirm = Some(Confirm::Type { text, reason }),
            None => self.deliver_text(text),
        }
    }

    /// Text for the target: inserted in one step through the agent when it is connected,
    /// otherwise typed key by key over the KVM.
    fn deliver_text(&mut self, text: String) {
        let insert = self.clip.as_ref().is_some_and(ClipBridge::can_insert)
            && text.len() <= cometdesk_clip::MAX_INSERT;
        if !insert {
            return self.start_typing(text);
        }
        let paste = self.device.insert_paste;
        let rx = self
            .clip
            .as_ref()
            .unwrap()
            .insert(text, paste, self.hid.clone());
        self.inserting = Some(rx);
    }

    /// Whether the clipboard button inserts through the agent (vs typing).
    fn inserts_via_agent(&self) -> bool {
        self.clip.as_ref().is_some_and(ClipBridge::can_insert)
    }

    /// Types `text` key by key. With a layout table for the target (`cometdesk::layout`) we send
    /// the keys ourselves, AltGr as Right Alt alone (Option on macOS); otherwise the device
    /// types it (`hid/print`), whose AltGr Linux and macOS misread.
    fn start_typing(&mut self, text: String) {
        if let Some(t) = self.typing.take() {
            t.task.abort();
        }
        let total = text.chars().count();
        let done = Arc::new(AtomicUsize::new(0));
        let (proxy, progress) = (self.proxy.clone(), done.clone());
        let task = if let Some(layout) = self.typing_layout() {
            let missing = layout.missing(&text);
            if !missing.is_empty() {
                let shown: String = missing.iter().take(10).collect();
                self.notify(format!(
                    "Skipping {} characters the {} layout cannot type: {shown}",
                    missing.len(),
                    layout.name
                ));
            }
            let hid = self.hid.clone();
            self.rt.spawn(async move {
                for (i, ch) in text.chars().enumerate() {
                    for cmd in layout.keys(ch).unwrap_or_default() {
                        hid.send(cmd);
                        tokio::time::sleep(KEY_GAP).await;
                    }
                    if i % 20 == 19 {
                        progress.store(i + 1, Ordering::Relaxed);
                        let _ = proxy.send_event(UserEvent::Refresh);
                    }
                }
                progress.store(total, Ordering::Relaxed);
                Ok(())
            })
        } else {
            let chunks: Vec<String> = actions::chunks(&text, actions::CHUNK_CHARS)
                .into_iter()
                .map(str::to_owned)
                .collect();
            let (session, keymap) = (self.session.clone(), self.keymap());
            self.rt.spawn(async move {
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
            })
        };
        self.typing = Some(Typing { total, done, task });
    }

    /// Our layout table for typing on this target, if we have one. Firmware screens read
    /// raw US scancodes.
    fn typing_layout(&self) -> Option<cometdesk::layout::Layout> {
        use cometdesk::layout::{Platform, layout};
        match self.device.target {
            TargetOs::Bios => layout("en-us", Platform::Windows),
            TargetOs::MacOs => layout(&self.keymap()?, Platform::Mac),
            TargetOs::Windows | TargetOs::Linux => layout(&self.keymap()?, Platform::Windows),
        }
    }

    /// Layout used to type text: the device's saved choice, else the device default.
    fn keymap(&self) -> Option<String> {
        self.device
            .keymap
            .clone()
            .or_else(|| self.default_keymap.clone())
    }

    /// Stops typing: dropping the request makes the device cancel and clear queued keys;
    /// keys we send ourselves may be held mid-character, so release them.
    fn cancel_typing(&mut self) {
        if let Some(t) = self.typing.take() {
            t.task.abort();
            self.hid.release_all();
            self.notify("Typing cancelled".into());
        }
    }

    /// `~/Downloads/cometdesk-<host>-<time>.<ext>`.
    fn download_path(&self, ext: &str) -> std::path::PathBuf {
        let dir = std::env::var_os("HOME")
            .map(|h| std::path::PathBuf::from(h).join("Downloads"))
            .unwrap_or_else(std::env::temp_dir);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        dir.join(format!(
            "cometdesk-{}-{stamp}.{ext}",
            self.device.host.replace(':', "_")
        ))
    }

    async fn save_notice(what: &str, path: &std::path::Path, bytes: &[u8]) -> String {
        match tokio::fs::write(path, bytes).await {
            Ok(()) => format!("{what} saved to {}", path.display()),
            Err(e) => format!("Cannot save {what}: {e}"),
        }
    }

    /// The device's snapshot (JPEG of the capture, before H.264).
    pub fn snapshot(&mut self) {
        let session = self.session.clone();
        let proxy = self.proxy.clone();
        let path = self.download_path("jpg");
        self.rt.spawn(async move {
            let msg = match session.snapshot(SnapshotOptions::default()).await {
                Ok(jpeg) => Self::save_notice("Snapshot", &path, &jpeg).await,
                Err(e) => format!("Snapshot failed: {e}"),
            };
            let _ = proxy.send_event(UserEvent::Notice(msg));
        });
    }

    /// A pixel-exact screenshot of the target's desktop (all monitors), taken by the agent.
    pub fn exact_screenshot(&mut self) {
        let Some(rx) = self.agent_capture(None) else {
            return;
        };
        let proxy = self.proxy.clone();
        let path = self.download_path("png");
        self.rt.spawn(async move {
            let msg = match tokio::time::timeout(CAPTURE_REPLY_TIMEOUT, rx).await {
                Ok(Ok(Ok(png))) => Self::save_notice("Screenshot", &path, &png).await,
                Ok(Ok(Err(e))) => format!("Screenshot failed: {e}"),
                Ok(Err(_)) => "Screenshot failed: the agent did not answer".into(),
                Err(_) => "Screenshot failed: the agent timed out".into(),
            };
            let _ = proxy.send_event(UserEvent::Notice(msg));
        });
    }

    fn agent_capture(
        &mut self,
        display: Option<u32>,
    ) -> Option<tokio::sync::oneshot::Receiver<crate::clipsync::CaptureResult>> {
        let rx = self
            .clip
            .as_ref()
            .filter(|c| c.can_insert())
            .map(|c| c.capture(display));
        if rx.is_none() {
            self.notify("No agent connected on the target".into());
        }
        rx
    }

    /// Starts picking a region of the video to copy as an image.
    pub fn pick_region(&mut self) {
        if self.video_pts.is_none() {
            return self.notify("No video to copy from yet".into());
        }
        self.release_all();
        self.region = Some(RegionPick {
            start: None,
            now: egui::Pos2::ZERO,
        });
    }

    /// Dims the video and lets the user drag a rectangle on it.
    fn region_overlay(&mut self, ui: &mut egui::Ui) {
        let (Some(pick), Some(area)) = (self.region.as_mut(), self.video_pts) else {
            return;
        };
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.region = None;
            return;
        }
        let resp = ui.interact(area, egui::Id::unique("region_pick"), egui::Sense::drag());
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Crosshair);
        if let Some(p) = resp.interact_pointer_pos() {
            pick.now = p;
            if resp.drag_started() {
                pick.start = Some(p);
            }
        }
        let painter = ui.painter();
        painter.rect_filled(area, 0.0, egui::Color32::from_black_alpha(90));
        painter.text(
            area.center_top() + egui::vec2(0.0, 12.0),
            egui::Align2::CENTER_TOP,
            "Drag over the part of the screen to copy · Esc cancels",
            egui::FontId::proportional(15.0),
            egui::Color32::WHITE,
        );
        let Some(start) = pick.start else { return };
        let sel = egui::Rect::from_two_pos(start, pick.now).intersect(area);
        painter.rect_stroke(
            sel,
            0.0,
            egui::Stroke::new(2.0, theme::OK),
            egui::StrokeKind::Inside,
        );
        if resp.drag_stopped() {
            self.region = None;
            if sel.width() >= 4.0 && sel.height() >= 4.0 {
                let to = |p: egui::Pos2| {
                    (
                        (p.x - area.min.x) / area.width(),
                        (p.y - area.min.y) / area.height(),
                    )
                };
                let (a, b) = (to(sel.min), to(sel.max));
                self.copy_screen_image([a.0, a.1, b.0, b.1]);
            }
        }
    }

    /// Copies part of the target's screen, `[left, top, right, bottom]` as fractions of the
    /// video, as an image: cut from the agent's exact screenshot of the main monitor.
    pub fn copy_screen_image(&mut self, region: [f32; 4]) {
        let Some(d) = self.agent().and_then(|(_, i)| {
            (i.session != SessionState::Locked)
                .then(|| i.displays.iter().position(|d| d.primary).unwrap_or(0) as u32)
        }) else {
            return self.notify("Needs the agent connected, with the target unlocked".into());
        };
        let Some(rx) = self.agent_capture(Some(d)) else {
            return;
        };
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            let msg = match tokio::time::timeout(CAPTURE_REPLY_TIMEOUT, rx).await {
                Ok(Ok(Ok(png))) => {
                    let image = tokio::task::spawn_blocking(move || {
                        let (w, h, rgba) = crate::crop::crop_png(&png, region)?;
                        arboard::Clipboard::new()
                            .and_then(|mut c| {
                                c.set_image(arboard::ImageData {
                                    width: w as usize,
                                    height: h as usize,
                                    bytes: rgba.into(),
                                })
                            })
                            .map(|()| (w, h))
                            .map_err(|e| e.to_string())
                    })
                    .await;
                    match image {
                        Ok(Ok((w, h))) => format!("Copied a {w}×{h} image of the screen"),
                        Ok(Err(e)) => format!("Cannot copy the screen: {e}"),
                        Err(e) => format!("Cannot copy the screen: {e}"),
                    }
                }
                Ok(Ok(Err(e))) => format!("Screenshot failed: {e}"),
                Ok(Err(_)) => "Screenshot failed: the agent did not answer".into(),
                Err(_) => "Screenshot failed: the agent timed out".into(),
            };
            let _ = proxy.send_event(UserEvent::Notice(msg));
        });
    }

    /// Recognizes the text on the target's screen and copies it. From the agent's exact
    /// screenshot of the main monitor when it is connected and the session is unlocked,
    /// else from the device snapshot.
    pub fn copy_screen_text(&mut self) {
        let primary = self.agent().and_then(|(_, i)| {
            (i.session != SessionState::Locked)
                .then(|| i.displays.iter().position(|d| d.primary).unwrap_or(0) as u32)
        });
        let exact = primary.and_then(|d| self.clip.as_ref().map(|c| c.capture(Some(d))));
        let session = self.session.clone();
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            let from_device = || async {
                session
                    .snapshot(SnapshotOptions::default())
                    .await
                    .map(|b| b.to_vec())
                    .map_err(|e| format!("Snapshot failed: {e}"))
            };
            let image = match exact {
                Some(rx) => match tokio::time::timeout(CAPTURE_REPLY_TIMEOUT, rx).await {
                    Ok(Ok(Ok(png))) => Ok(png),
                    _ => from_device().await,
                },
                None => from_device().await,
            };
            let msg = match image {
                Err(e) => e,
                Ok(img) => match tokio::task::spawn_blocking(move || crate::ocr::recognize(&img))
                    .await
                {
                    Ok(Ok(text)) if text.trim().is_empty() => "No text found on the screen".into(),
                    Ok(Ok(text)) => {
                        let n = text.chars().count();
                        match arboard::Clipboard::new().and_then(|mut c| c.set_text(text)) {
                            Ok(()) => format!("Copied {n} characters of screen text"),
                            Err(e) => format!("Cannot copy: {e}"),
                        }
                    }
                    Ok(Err(e)) => format!("Text recognition failed: {e}"),
                    Err(e) => format!("Text recognition failed: {e}"),
                },
            };
            let _ = proxy.send_event(UserEvent::Notice(msg));
        });
    }

    /// What the agent reports, as a menu: for checking at a glance that the target is what
    /// the app assumes.
    fn info_menu(&mut self, ui: &mut egui::Ui) {
        let Some((hello, info)) = self.agent() else {
            return;
        };
        let up = match (info.uptime_secs, self.uptime_seen) {
            (Some(u), Some((seen, at))) if seen == u => Some(u + at.elapsed().as_secs()),
            (u, _) => u,
        };
        // A fixed title: load figures live in the body so the menu does not change width.
        let (title, tint) = match info.session {
            SessionState::Locked => ("System info · locked".to_string(), Some(theme::WARN)),
            _ => ("System info".to_string(), None),
        };
        let mut title = egui::RichText::new(title);
        if let Some(c) = tint {
            title = title.color(c);
        }
        theme::section(ui, title, |ui| {
            let layouts: Vec<&str> = info
                .layouts
                .iter()
                .filter_map(|l| cometdesk::layout::keymap_from_native(l))
                .collect();
            let mut rows: Vec<(&str, String)> = vec![
                ("Host", hello.name.clone()),
                ("OS", info.os_version.clone().unwrap_or(hello.os.clone())),
                ("User", info.user.clone().unwrap_or_default()),
                (
                    "Session",
                    match info.session {
                        SessionState::Locked => "locked (lock screen)".into(),
                        SessionState::Active => "active".into(),
                        SessionState::Unknown => "unknown".into(),
                    },
                ),
                ("Keyboard", info.keyboard.clone().unwrap_or_default()),
                ("Layouts", layouts.join(", ")),
            ];
            rows.push((
                "Input method",
                match info.ime {
                    Some(true) => "on (typing key by key may be composed)".into(),
                    Some(false) => "off".into(),
                    None => String::new(),
                },
            ));
            if let Some(f) = &info.focus {
                rows.push((
                    "Focus",
                    format!("{}{}", f.app, if f.terminal { " (terminal)" } else { "" }),
                ));
            }
            for (i, d) in info.displays.iter().enumerate() {
                rows.push((
                    "Display",
                    format!(
                        "{}: {}x{} at {},{} · {}%{}",
                        i + 1,
                        d.width,
                        d.height,
                        d.x,
                        d.y,
                        d.scale,
                        if d.primary { " · main" } else { "" }
                    ),
                ));
            }
            if let Some(u) = up {
                rows.push(("Uptime", format!("{}h {:02}m", u / 3600, u / 60 % 60)));
            }
            if let Some(b) = info.battery {
                rows.push((
                    "Battery",
                    format!(
                        "{}%{}",
                        b.percent,
                        if b.charging { " (charging)" } else { "" }
                    ),
                ));
            }
            if let Some(l) = info.load {
                let usage = |u: cometdesk_clip::Usage| {
                    format!(
                        "{:.1} / {:.1} GB ({}%)",
                        u.used_mb as f64 / 1024.0,
                        u.total_mb as f64 / 1024.0,
                        (u.used_mb * 100).checked_div(u.total_mb).unwrap_or(0)
                    )
                };
                if let Some(c) = l.cpu_percent {
                    rows.push(("CPU", format!("{c}%")));
                }
                if let Some(m) = l.memory {
                    rows.push(("Memory", usage(m)));
                }
                if let Some(d) = l.disk {
                    rows.push(("Disk", usage(d)));
                }
            }
            egui::Grid::new("agent_info").num_columns(2).show(ui, |ui| {
                for (k, v) in rows.iter().filter(|(_, v)| !v.is_empty()) {
                    ui.label(*k);
                    ui.label(v);
                    ui.end_row();
                }
            });
            if let Some(v) = info.volume {
                ui.separator();
                self.volume_row(ui, v);
            }
            for (i, d) in info.displays.iter().enumerate() {
                self.display_menu(ui, i, d, info.displays.len());
            }
            if info.layouts.len() > 1 {
                ui.separator();
                theme::section(ui, "Switch the target's layout", |ui| {
                    for l in &info.layouts {
                        let label = cometdesk::layout::keymap_from_native(l).unwrap_or(l);
                        let active = info.keyboard.as_deref() == Some(l.as_str());
                        if ui.selectable_label(active, label).clicked() && !active {
                            self.request_layout(l.clone());
                            ui.close();
                        }
                    }
                });
            }
        });
    }

    /// Lock, sleep, log off, restart and shut down through the agent: the OS closes the
    /// session cleanly, unlike the ATX button. Shown only while the agent is connected.
    fn session_menu(&mut self, ui: &mut egui::Ui) {
        if !self.inserts_via_agent() {
            return;
        }
        theme::section(ui, "Session", |ui| {
            for a in [
                SessionAction::Lock,
                SessionAction::Sleep,
                SessionAction::Logoff,
                SessionAction::Restart,
                SessionAction::Shutdown,
            ] {
                let label = if a.destructive() {
                    format!("{}…", a.label())
                } else {
                    a.label().to_string()
                };
                if ui.button(label).clicked() {
                    if a.destructive() {
                        self.confirm = Some(Confirm::Session(a));
                    } else {
                        self.session_action(a);
                    }
                    ui.close();
                }
            }
        });
    }

    /// A message from the target: a toast while the window is in front, else a system
    /// notification (a toast if the platform has none).
    fn target_note(&mut self, title: &str, body: &str) {
        let host = self.device.host.clone();
        if !self.window_focused && crate::sysnotify::show(title, body, &host) {
            return;
        }
        self.notify(if body.is_empty() {
            title.to_owned()
        } else {
            format!("{title}: {body}")
        });
    }

    pub fn set_window_focused(&mut self, focused: bool) {
        self.window_focused = focused;
    }

    /// Streams `paths` to the target's Downloads folder through the agent.
    fn send_files(&mut self, paths: Vec<std::path::PathBuf>) {
        let Some(clip) = self.clip.as_ref() else {
            return;
        };
        let mut errors = Vec::new();
        for path in paths {
            match clip.send_file(path.clone()) {
                Ok(t) => self.transfers.push(t),
                Err(e) => errors.push(format!("Cannot send {}: {e}", path.display())),
            }
        }
        for e in errors {
            self.notify(e);
        }
    }

    fn pick_files_to_send(&self) {
        let (picked, proxy) = (self.picked_files.clone(), self.proxy.clone());
        self.rt.spawn(async move {
            let files = rfd::AsyncFileDialog::new()
                .set_title("Send files to the target")
                .pick_files()
                .await;
            if let Some(files) = files {
                picked
                    .lock()
                    .unwrap()
                    .extend(files.iter().map(|f| f.path().to_owned()));
            }
            let _ = proxy.send_event(UserEvent::Refresh);
        });
    }

    /// Reports finished transfers and draws the running ones.
    fn poll_transfers(&mut self, ctx: &egui::Context) {
        use tokio::sync::oneshot::error::TryRecvError;
        let mut notes = Vec::new();
        self.transfers.retain_mut(|t| match t.result.try_recv() {
            Ok(Ok(path)) => {
                notes.push(format!("Sent {} to the target: {path}", t.progress.name));
                false
            }
            Ok(Err(e)) => {
                notes.push(format!("Sending {} failed: {e}", t.progress.name));
                false
            }
            Err(TryRecvError::Closed) => {
                notes.push(format!("Sending {} was interrupted", t.progress.name));
                false
            }
            Err(TryRecvError::Empty) => true,
        });
        for n in notes {
            self.notify(n);
        }
        if self.transfers.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::unique("transfers"))
            .anchor(egui::Align2::LEFT_BOTTOM, [8.0, -8.0])
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    for p in self.transfers.iter().map(|t| &t.progress) {
                        let sent = p.sent.load(Ordering::Relaxed);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::ProgressBar::new(sent as f32 / p.total.max(1) as f32)
                                    .desired_width(240.0)
                                    .text(format!(
                                        "{} · {:.1} / {:.1} MB",
                                        p.name,
                                        sent as f64 / 1e6,
                                        p.total as f64 / 1e6
                                    )),
                            );
                            if ui.button("Cancel").clicked() {
                                p.cancel.store(true, Ordering::Relaxed);
                            }
                        });
                    }
                });
            });
    }

    fn request_layout(&mut self, layout: String) {
        self.ask_agent(Request::Layout(layout), "Cannot switch the layout", None);
    }

    /// Sends `request` to the agent; `failure` prefixes the error shown if it says no, and
    /// `done` is shown if it says yes.
    fn ask_agent(&mut self, request: Request, failure: &'static str, done: Option<String>) {
        let Some(rx) = self.clip.as_ref().map(|c| c.request(request)) else {
            return;
        };
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            let msg = match rx.await {
                Ok(Ok(())) => match done {
                    Some(m) => m,
                    None => return,
                },
                Ok(Err(e)) => format!("{failure}: {e}"),
                Err(_) => format!("{failure}: the agent disconnected"),
            };
            let _ = proxy.send_event(UserEvent::Notice(msg));
        });
    }

    /// The master volume of the target: a slider that sends on release, and mute.
    fn volume_row(&mut self, ui: &mut egui::Ui, volume: cometdesk_clip::Volume) {
        let shown = self
            .volume_edit
            .filter(|(_, at)| at.elapsed() < Duration::from_secs(3))
            .map_or(volume.percent, |(v, _)| v);
        let mut level = f32::from(shown);
        let mut muted = volume.muted;
        ui.horizontal(|ui| {
            ui.label("Volume");
            let slider = ui.add(egui::Slider::new(&mut level, 0.0..=100.0).suffix("%"));
            if slider.changed() {
                self.volume_edit = Some((level as u8, Instant::now()));
            }
            if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
                self.ask_agent(
                    Request::Volume {
                        level: Some(level as u8),
                        muted: None,
                    },
                    "Cannot set the volume",
                    None,
                );
            }
            if ui.checkbox(&mut muted, "Mute").changed() {
                self.ask_agent(
                    Request::Volume {
                        level: None,
                        muted: Some(muted),
                    },
                    "Cannot change the mute",
                    None,
                );
            }
        });
    }

    /// Resolution and main-display choices for display `index` of the target.
    fn display_menu(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        d: &cometdesk_clip::Display,
        count: usize,
    ) {
        let can_main = count > 1 && !d.primary;
        if d.modes.is_empty() && !can_main {
            return;
        }
        theme::section(ui, format!("Display {}", index + 1), |ui| {
            if !d.modes.is_empty() {
                ui.label("Resolution");
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for &(w, h) in &d.modes {
                            let on = (w, h) == (d.width, d.height);
                            if ui.selectable_label(on, format!("{w}×{h}")).clicked() && !on {
                                self.ask_agent(
                                    Request::Display {
                                        index: index as u32,
                                        mode: Some((w, h)),
                                        primary: false,
                                    },
                                    "Cannot change the resolution",
                                    Some(format!("Resolution set to {w}×{h}")),
                                );
                                ui.close();
                            }
                        }
                    });
                // The KVM's EDID tells the target which modes the monitor has: when the
                // target runs a mode the current EDID does not name, offer the matching one.
                if let Some(p) = self.edid_for(d.width, d.height)
                    && ui
                        .button(format!("Match the EDID: {}", p.label))
                        .on_hover_text("Tell the target the monitor is this one")
                        .clicked()
                {
                    self.apply_video(VideoChange::Edid(p));
                    ui.close();
                }
            }
            if can_main && ui.button("Make this the main display").clicked() {
                self.ask_agent(
                    Request::Display {
                        index: index as u32,
                        mode: None,
                        primary: true,
                    },
                    "Cannot change the main display",
                    Some(format!("Display {} is now the main display", index + 1)),
                );
                ui.close();
            }
        });
    }

    /// The device's EDID preset named for `width`×`height` (or its `…p`), if it is not the
    /// one in use.
    fn edid_for(&self, width: u32, height: u32) -> Option<EdidPreset> {
        let needles = [
            format!("{width}x{height}"),
            format!("{width}×{height}"),
            format!("{height}p"),
        ];
        let current = &self.video_info.edid;
        self.video_info
            .edid_presets
            .iter()
            .filter(|p| edid::normalize(&p.content) != *current)
            .find(|p| {
                let label = p.label.to_lowercase();
                needles.iter().any(|n| label.contains(n.as_str()))
            })
            .cloned()
    }

    fn session_action(&mut self, action: SessionAction) {
        let Some(rx) = self
            .clip
            .as_ref()
            .map(|c| c.request(Request::Power(action)))
        else {
            return;
        };
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            let msg = match rx.await {
                Ok(Ok(())) => format!("{}: requested", action.label()),
                Ok(Err(e)) => format!("{} failed: {e}", action.label()),
                // The target went down before it could answer.
                Err(_) => format!("{}: the agent disconnected", action.label()),
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
                _ => return,
            };
            let msg = match r {
                Ok(()) => format!("ATX {label}: done"),
                Err(e) => format!("ATX {label} failed: {e}"),
            };
            let _ = proxy.send_event(UserEvent::Notice(msg));
        });
    }

    /// Tears the console down and logs out in the background; the returned handle completes
    /// when the logout does, so a quitting caller can wait for it.
    pub fn disconnect(mut self) -> JoinHandle<()> {
        self.release_all();
        if let Some(w) = self.watcher.take() {
            w.abort();
        }
        self.stream = None;
        self.clip = None;
        if let Some(t) = self.typing.take() {
            t.task.abort();
        }
        self.media.shutdown();
        let session = self.session.clone();
        let events = self.events.take();
        self.rt.spawn(async move {
            if let Some(e) = events {
                e.close().await;
            }
            let _ = session.logout().await;
        })
    }

    // ----- ui --------------------------------------------------------------------------

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
                    // WebRTC sources get the angle in their handshake: restart to apply it.
                    let rotated = info.orientation != self.video_info.orientation
                        && self.video_info.mode.is_some()
                        && self.transport != Transport::Direct;
                    let codec_changed =
                        info.codec != self.video_info.codec && self.video_info.mode.is_some();
                    self.video_info = info;
                    self.video_info_rx = None;
                    if rotated || codec_changed {
                        self.stream = None;
                        self.start_stream();
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
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .inner_margin(egui::Margin::symmetric(12, 6));
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
                egui::Stroke::new(1.5, signal),
            );
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

    fn status(&self) -> (String, egui::Color32) {
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

    fn toolbar(&mut self, ui: &mut egui::Ui) -> ConsoleAction {
        let mut action = ConsoleAction::None;

        // Left: where am I, and is it working.
        if ui.button("‹ Devices").clicked() {
            action = ConsoleAction::Disconnect;
        }
        theme::badge(ui, &self.device.host, self.device.display_name(), 26.0);
        ui.label(
            egui::RichText::new(self.device.display_name())
                .size(17.0)
                .strong(),
        );
        let (text, color) = self.status();
        // Fixed width: a status change must not shift the controls under the pointer.
        ui.allocate_ui_with_layout(
            egui::vec2(STATUS_WIDTH, ui.spacing().interact_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| theme::status_dot(ui, color, &text),
        );
        ui.separator();

        // Do things on the target.
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
        let hint = if self.inserts_via_agent() {
            "Insert the clipboard in one step through the agent on the target (Right Cmd + V)"
        } else {
            "Type the clipboard key by key: no agent connected on the target (Right Cmd + V)"
        };
        if ui
            .add(egui::Button::new("Paste"))
            .on_hover_text(hint)
            .clicked()
        {
            self.request_type_clipboard();
        }
        let agent_up = self.inserts_via_agent();
        ui.menu_button("Capture", |ui| {
            if ui
                .button("Save device snapshot")
                .on_hover_text("JPEG from the KVM capture, saved to ~/Downloads")
                .clicked()
            {
                self.snapshot();
                ui.close();
            }
            if ui
                .add_enabled(agent_up, egui::Button::new("Save exact screenshot"))
                .on_hover_text("Pixel-exact, taken by the agent on the target (all monitors)")
                .on_disabled_hover_text("Needs the agent connected on the target")
                .clicked()
            {
                self.exact_screenshot();
                ui.close();
            }
            if ui
                .add_enabled(agent_up, egui::Button::new("Copy region as image"))
                .on_hover_text(
                    "Drag a rectangle on the video; copied pixel-exact from the agent's screenshot",
                )
                .on_disabled_hover_text("Needs the agent connected on the target")
                .clicked()
            {
                self.pick_region();
                ui.close();
            }
            if ui
                .add_enabled(agent_up, egui::Button::new("Copy screenshot as image"))
                .on_hover_text("The main monitor, pixel-exact, from the agent")
                .on_disabled_hover_text("Needs the agent connected on the target")
                .clicked()
            {
                self.copy_screen_image([0.0, 0.0, 1.0, 1.0]);
                ui.close();
            }
            if ui
                .button("Copy screen text (OCR)")
                .on_hover_text(
                    "Recognized on this Mac; from the agent's exact screenshot when available",
                )
                .clicked()
            {
                self.copy_screen_text();
                ui.close();
            }
        });
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

        // Right: the viewer itself.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (icon, tip) = if self.fullscreen {
                ("Exit fullscreen", "Right Cmd + F")
            } else {
                ("Fullscreen", "Right Cmd + F")
            };
            if ui.button(icon).on_hover_text(tip).clicked() {
                action = ConsoleAction::ToggleFullscreen;
            }
            let panel_open = self.drawer.is_some();
            if ui
                .selectable_label(panel_open, "Panel")
                .on_hover_text("Target, video and settings")
                .clicked()
            {
                self.drawer = if panel_open { None } else { Some(Tab::Target) };
            }
            if ui
                .button("Actions")
                .on_hover_text("Search every action (Right Cmd + K)")
                .clicked()
            {
                self.palette = Some(String::new());
            }
        });
        action
    }

    /// Everything about the machine behind the KVM: clipboard sync, files, session, system
    /// info and virtual media. True when the saved device changed.
    fn target_panel(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        let locked = self
            .agent()
            .is_some_and(|(_, i)| matches!(i.session, SessionState::Locked));
        if locked {
            theme::banner(ui, theme::WARN, "The target is locked (lock screen).");
        } else if self.clip.is_some() && self.bridge_action().is_some() {
            theme::banner(
                ui,
                theme::WARN,
                "Clipboard needs attention: open Clipboard below.",
            );
        }
        if self.clip_menu(ui) != ConsoleAction::None {
            changed = true;
        }
        if self.inserts_via_agent() {
            if ui
                .button("Send file…")
                .on_hover_text(
                    "Copy a file to the target's Downloads folder (or drop it on the window)",
                )
                .clicked()
            {
                self.pick_files_to_send();
            }
            if ui
                .button("Browse files…")
                .on_hover_text("Browse the target's files and download them to this Mac")
                .clicked()
                && let Some(clip) = self.clip.as_ref()
            {
                self.browser.open(clip);
            }
        }
        self.session_menu(ui);
        self.info_menu(ui);
        if self.caps.supports(Feature::Msd) {
            self.media.menu(ui);
        }
        changed
    }

    /// Target OS, keyboard layout, stats and the device's web interface. True when the saved
    /// device changed.
    fn settings_panel(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        {
            ui.set_min_width(200.0);
            let before = self.device.target;
            theme::section(ui, format!("Target OS: {}", before.label()), |ui| {
                for os in TargetOs::ALL {
                    if ui
                        .radio_value(&mut self.device.target, os, os.label())
                        .clicked()
                    {
                        ui.close();
                    }
                }
            });
            if self.device.target != before {
                let iso = self.mapper.keys.iso_fix;
                self.mapper.keys = self.device.target.key_options(iso);
                changed = true;
            }

            let current = self.keymap().unwrap_or_else(|| "default".into());
            let on_target: Vec<&'static str> = self
                .agent()
                .map(|(_, i)| {
                    i.layouts
                        .iter()
                        .filter_map(|l| cometdesk::layout::keymap_from_native(l))
                        .collect()
                })
                .unwrap_or_default();
            let before = self.device.keymap.clone();
            theme::section(ui, format!("Keyboard layout: {current}"), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for k in &self.keymaps {
                            let label = if on_target.contains(&k.as_str()) {
                                format!("{k} (on target)")
                            } else {
                                k.clone()
                            };
                            if ui
                                .radio_value(&mut self.device.keymap, Some(k.clone()), label)
                                .clicked()
                            {
                                ui.close();
                            }
                        }
                    });
            })
            .on_hover_text("Keyboard layout of the target, used by Paste when typing");
            changed |= self.device.keymap != before;

            ui.separator();
            ui.checkbox(&mut self.show_stats, "Show stats")
                .on_hover_text("Right Cmd + S");
            if ui
                .button("Device web interface…")
                .on_hover_text("Opens the device's web interface in the browser")
                .clicked()
            {
                let url = format!("https://{}/", self.session.host());
                if let Err(e) = open::that(&url) {
                    self.notify(format!("Cannot open {url}: {e}"));
                }
                ui.close();
            }
        }
        changed
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
                egui::Label::new(egui::RichText::new(msg).size(18.0).color(theme::MUTED)),
            );
        }
        self.region_overlay(ui);
        // Fullscreen hides the toolbar and its status: surface anything that is not OK.
        let (text, color) = self.status();
        if self.fullscreen && self.toolbar_rect.is_none() && color != theme::OK {
            egui::Area::new(egui::Id::unique("fullscreen-status"))
                .anchor(egui::Align2::CENTER_TOP, [0.0, 12.0])
                .interactable(false)
                .show(ui.ctx(), |ui| theme::floating_status(ui, color, &text));
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

    /// The right-hand panel: everything that is not one of the four everyday actions.
    /// True when the saved device changed.
    fn drawer_ui(&mut self, ctx: &egui::Context) -> bool {
        let Some(tab) = self.drawer else { return false };
        let top = self.toolbar_rect.map_or(0.0, |r| r.bottom());
        let height = (ctx.content_rect().height() - top - 24.0).max(240.0);
        let (mut changed, mut close, mut next) = (false, false, tab);
        #[allow(deprecated)] // a stable id keeps the panel's scroll position
        let panel_id = egui::Id::new("panel");
        egui::Area::new(panel_id)
            .anchor(egui::Align2::RIGHT_TOP, [-8.0, top + 8.0])
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(theme::PANEL)
                    .stroke(egui::Stroke::new(1.0, theme::BORDER))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(320.0);
                        ui.set_min_height(height - 24.0);
                        theme::set_inline_sections(ui, true);
                        ui.horizontal(|ui| {
                            for t in Tab::ALL {
                                if ui.selectable_label(tab == t, t.label()).clicked() {
                                    next = t;
                                }
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    close = ui.add(theme::ghost("Close")).clicked();
                                },
                            );
                        });
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .max_height(height - 64.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| match tab {
                                Tab::Target => changed = self.target_panel(ui),
                                Tab::Video => self.video_menu(ui),
                                Tab::Settings => changed = self.settings_panel(ui),
                            });
                    });
            });
        self.drawer = if close { None } else { Some(next) };
        changed
    }

    fn commands(&self) -> Vec<(String, Cmd)> {
        let agent = self.inserts_via_agent();
        let mut v = vec![("Paste the clipboard".to_string(), Cmd::Paste)];
        for (label, keys) in actions::COMBOS {
            v.push((format!("Send {label}"), Cmd::Keys(keys)));
        }
        if self.caps.supports(Feature::Atx) {
            v.extend([
                ("Power on".into(), Cmd::Power(PowerAction::On)),
                ("Shut down".into(), Cmd::Power(PowerAction::Off)),
                ("Force power off".into(), Cmd::Power(PowerAction::OffHard)),
                ("Reset".into(), Cmd::Power(PowerAction::ResetHard)),
                (
                    "Press the power button".into(),
                    Cmd::Click(AtxButton::Power),
                ),
            ]);
        }
        v.push(("Save device snapshot".into(), Cmd::Snapshot));
        v.push(("Copy screen text (OCR)".into(), Cmd::Ocr));
        if agent {
            v.extend([
                ("Save exact screenshot".into(), Cmd::ExactShot),
                ("Copy region as image".into(), Cmd::CopyRegion),
                ("Copy screenshot as image".into(), Cmd::CopyScreen),
                ("Send file…".into(), Cmd::SendFile),
                ("Browse the target's files…".into(), Cmd::BrowseFiles),
            ]);
        }
        v.extend([
            ("Toggle fullscreen".into(), Cmd::Fullscreen),
            ("Toggle stats".into(), Cmd::Stats),
            ("Open panel: Target".into(), Cmd::Panel(Tab::Target)),
            ("Open panel: Video".into(), Cmd::Panel(Tab::Video)),
            ("Open panel: Settings".into(), Cmd::Panel(Tab::Settings)),
            ("Open the device web interface".into(), Cmd::WebUi),
            ("Back to devices".into(), Cmd::Disconnect),
        ]);
        v
    }

    fn run_command(&mut self, cmd: Cmd) -> ConsoleAction {
        match cmd {
            Cmd::Paste => self.request_type_clipboard(),
            Cmd::Keys(keys) => self.send_combo(keys),
            Cmd::Power(a) => self.confirm = Some(Confirm::Power(a)),
            Cmd::Click(b) => self.confirm = Some(Confirm::Click(b)),
            Cmd::Snapshot => self.snapshot(),
            Cmd::ExactShot => self.exact_screenshot(),
            Cmd::CopyRegion => self.pick_region(),
            Cmd::CopyScreen => self.copy_screen_image([0.0, 0.0, 1.0, 1.0]),
            Cmd::Ocr => self.copy_screen_text(),
            Cmd::Fullscreen => return ConsoleAction::ToggleFullscreen,
            Cmd::Stats => self.show_stats = !self.show_stats,
            Cmd::Panel(t) => self.drawer = Some(t),
            Cmd::SendFile => self.pick_files_to_send(),
            Cmd::BrowseFiles => {
                if let Some(clip) = self.clip.as_ref() {
                    self.browser.open(clip);
                }
            }
            Cmd::WebUi => {
                let url = format!("https://{}/", self.session.host());
                if let Err(e) = open::that(&url) {
                    self.notify(format!("Cannot open {url}: {e}"));
                }
            }
            Cmd::Disconnect => return ConsoleAction::Disconnect,
        }
        ConsoleAction::None
    }

    /// The action search: type to filter, Up/Down to move, Enter to run, Esc to close.
    fn palette_ui(&mut self, ctx: &egui::Context) -> ConsoleAction {
        let Some(mut query) = self.palette.take() else {
            return ConsoleAction::None;
        };
        let needle = query.to_lowercase();
        let mut matches: Vec<(String, Cmd)> = self
            .commands()
            .into_iter()
            .filter(|(l, _)| l.to_lowercase().contains(&needle))
            .collect();
        #[allow(deprecated)]
        let sel_id = egui::Id::new("palette-selected");
        let mut sel = ctx.data(|d| d.get_temp::<usize>(sel_id)).unwrap_or(0);
        let (down, up, close) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.key_pressed(egui::Key::Escape),
            )
        });
        let last = matches.len().saturating_sub(1);
        if down {
            sel = (sel + 1).min(last);
        }
        if up {
            sel = sel.saturating_sub(1);
        }
        let before = query.clone();
        let mut chosen = None;
        egui::Modal::new(egui::Id::unique("actions")).show(ctx, |ui| {
            ui.set_width(440.0);
            let edit = ui.add(
                egui::TextEdit::singleline(&mut query)
                    .desired_width(f32::INFINITY)
                    .hint_text("Search actions…"),
            );
            edit.request_focus();
            ui.add_space(4.0);
            if matches.is_empty() {
                ui.label(egui::RichText::new("No action matches.").color(theme::MUTED));
            }
            egui::ScrollArea::vertical()
                .max_height(340.0)
                .show(ui, |ui| {
                    for (i, (label, cmd)) in matches.iter().enumerate() {
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 28.0),
                            egui::Sense::click(),
                        );
                        let on = i == sel;
                        if on || resp.hovered() {
                            let fill = if on { theme::CARD_HOVER } else { theme::CARD };
                            ui.painter()
                                .rect_filled(rect, egui::CornerRadius::same(4), fill);
                        }
                        if on {
                            ui.painter().rect_stroke(
                                rect,
                                egui::CornerRadius::same(4),
                                egui::Stroke::new(1.0, theme::ACCENT),
                                egui::StrokeKind::Inside,
                            );
                            resp.scroll_to_me(None);
                        }
                        let font = egui::TextStyle::Body.resolve(ui.style());
                        ui.painter().text(
                            rect.left_center() + egui::vec2(10.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            label,
                            font.clone(),
                            theme::TEXT,
                        );
                        ui.painter().text(
                            rect.right_center() - egui::vec2(10.0, 0.0),
                            egui::Align2::RIGHT_CENTER,
                            cmd.hint(),
                            egui::TextStyle::Small.resolve(ui.style()),
                            theme::MUTED,
                        );
                        if resp.clicked() {
                            chosen = Some(i);
                        }
                    }
                });
            if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                chosen = (!matches.is_empty()).then_some(sel.min(last));
            }
        });
        if query != before {
            sel = 0;
        }
        ctx.data_mut(|d| d.insert_temp(sel_id, sel));
        if let Some(i) = chosen {
            ctx.data_mut(|d| d.insert_temp(sel_id, 0usize));
            let (_, cmd) = matches.swap_remove(i);
            return self.run_command(cmd);
        }
        if !close {
            self.palette = Some(query);
        } else {
            ctx.data_mut(|d| d.insert_temp(sel_id, 0usize));
        }
        ConsoleAction::None
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        self.target_setup_dialog(ctx);
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
            Confirm::Session(a) => (
                format!("{} the target?", a.label()),
                match a {
                    SessionAction::Logoff => "Programs on the target are closed; unsaved work may be lost.",
                    SessionAction::Restart => "The target restarts; unsaved work may be lost, and the agent reconnects after boot.",
                    _ => "The target powers off; unsaved work may be lost. Turn it back on with the ATX power menu.",
                }
                .into(),
            ),
            Confirm::BridgeSetup(what) => (
                format!("{what}?"),
                "Installs the clipboard bridge on the Comet through its web terminal and \
                 enables its USB network link to the target. If that link has to be (re)created, \
                 the target's keyboard and mouse disconnect for a moment. Takes about 20 seconds."
                    .into(),
            ),
            Confirm::Video(c) => match c {
                VideoChange::Mode(m) => (
                    format!("Switch the video mode to {}?", m.wire_name()),
                    "This is saved on the device and affects every viewer, including the web \
                     interface. Other viewers may need to reconnect."
                        .into(),
                ),
                VideoChange::Encoder(u) => (
                    format!(
                        "Set the encoder to {}?",
                        [
                            u.desired_fps.map(|v| format!("{v} fps")),
                            u.h264_bitrate.map(|v| format!("{v} kbit/s")),
                            u.h264_gop.map(|v| format!("a keyframe every {v} frames")),
                        ]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(", ")
                    ),
                    "This is saved on the device and affects every viewer's bitrate and frame rate."
                        .into(),
                ),
                VideoChange::Codec(f) => (
                    format!(
                        "Switch the video codec to {}?",
                        f.label()
                    ),
                    "This is saved on the device and affects every viewer. Switching codec restarts \
                     the encoder for a few seconds. WebRTC with error correction can't carry H.265; \
                     viewers on it switch to Direct."
                        .into(),
                ),
                VideoChange::Venc(m) => (
                    format!("Switch the encoder to {} mode?", m.wire_name()),
                    "This is saved on the device and affects every viewer.".into(),
                ),
                VideoChange::Latency(m) => (
                    format!("Switch to {}?", m.wire_name().replace('_', " ")),
                    "This is saved on the device and affects every viewer.".into(),
                ),
                VideoChange::Orientation(o) => (
                    format!("Rotate the picture to {}°?", o.degrees()),
                    "This is saved on the device and affects every viewer, including the web \
                     interface."
                        .into(),
                ),
                VideoChange::Edid(p) => (
                    format!("Present the target a {} display?", p.label),
                    "The target sees a monitor change and may rearrange its windows or \
                     resolution. This is saved on the device and affects every viewer."
                        .into(),
                ),
            },
        };
        let mut decision = None;
        egui::Modal::new(egui::Id::unique("confirm")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(egui::RichText::new(title).size(17.0).strong());
            ui.label(egui::RichText::new(body).color(theme::MUTED));
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    decision = Some(false);
                }
                if ui.add(theme::primary("Continue")).clicked() {
                    decision = Some(true);
                }
            });
        });
        match decision {
            Some(true) => match self.confirm.take() {
                Some(Confirm::Type { text, .. }) => self.deliver_text(text),
                Some(Confirm::BridgeSetup(_)) => self.setup.run(),
                Some(Confirm::Video(c)) => self.apply_video(c),
                Some(Confirm::Session(a)) => self.session_action(a),
                Some(c) => self.atx(c),
                None => {}
            },
            Some(false) => self.confirm = None,
            None => {}
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Target,
    Video,
    Settings,
}

impl Tab {
    const ALL: [Tab; 3] = [Tab::Target, Tab::Video, Tab::Settings];

    fn label(self) -> &'static str {
        match self {
            Tab::Target => "Target",
            Tab::Video => "Video",
            Tab::Settings => "Settings",
        }
    }
}

/// An entry of the action search.
enum Cmd {
    Paste,
    Keys(&'static [&'static str]),
    Power(PowerAction),
    Click(AtxButton),
    Snapshot,
    ExactShot,
    CopyRegion,
    CopyScreen,
    Ocr,
    Fullscreen,
    Stats,
    Panel(Tab),
    SendFile,
    BrowseFiles,
    WebUi,
    Disconnect,
}

impl Cmd {
    /// Right-hand note in the action search: the shortcut, or the group it belongs to.
    fn hint(&self) -> &'static str {
        match self {
            Cmd::Paste => "Right Cmd V",
            Cmd::Fullscreen => "Right Cmd F",
            Cmd::Stats => "Right Cmd S",
            Cmd::Keys(_) => "Keys",
            Cmd::Power(_) | Cmd::Click(_) => "Power",
            Cmd::Snapshot | Cmd::ExactShot | Cmd::CopyRegion | Cmd::CopyScreen | Cmd::Ocr => {
                "Capture"
            }
            Cmd::SendFile | Cmd::BrowseFiles => "Files",
            Cmd::Panel(_) | Cmd::WebUi | Cmd::Disconnect => "Viewer",
        }
    }
}

/// Width reserved for the status pill in the toolbar: fits the longest status text.
const STATUS_WIDTH: f32 = 196.0;

/// How long the pointer rests on the top edge before the fullscreen toolbar appears.
const EDGE_DWELL: std::time::Duration = std::time::Duration::from_millis(350);

fn video_mode_label(m: VideoMode) -> &'static str {
    match m {
        VideoMode::FlexFec => "WebRTC with error correction",
        VideoMode::Webrtc => "WebRTC",
        VideoMode::GlWebrtc => "GL native (no Janus)",
        VideoMode::DirectH264 => "Direct H.264",
    }
}

fn health_text(h: &Health) -> String {
    match h {
        Health::NotInstalled => "the Comet has no clipboard bridge".into(),
        Health::Outdated { installed } => format!("the Comet's bridge is outdated ({installed})"),
        Health::Broken(why) => format!("the Comet's bridge is broken: {why}"),
        Health::Ok => "the Comet's bridge is ready".into(),
    }
}

/// Agent binary the target installer downloads for `os`.
fn agent_name(os: TargetOs) -> Option<&'static str> {
    match os {
        TargetOs::Linux => Some("linux-x86_64"),
        TargetOs::Windows => Some("windows-x86_64.exe"),
        TargetOs::MacOs | TargetOs::Bios => None,
    }
}

/// The one command that installs, pairs and starts the agent (bridge/target/install.*).
///
/// It is typed over HID in the target's layout, so it avoids characters that need AltGr on
/// common layouts (`|`, `[`, `]`, `{`, `}`, `\`, `@`, `~` on German, for one): those may not
/// arrive. Hence a download to a file instead of a pipe, and `/` in Windows paths.
fn install_command(os: TargetOs, code: &str) -> Option<String> {
    const URL: &str = "http://192.168.5.1:47475";
    match os {
        TargetOs::Linux => Some(format!(
            "curl -fsSo /tmp/cometdesk-install.sh {URL}/install.sh && sh /tmp/cometdesk-install.sh {code}"
        )),
        TargetOs::Windows => Some(format!(
            "powershell -NoProfile -ExecutionPolicy Bypass -Command \"iwr -UseBasicParsing {URL}/install.ps1 -OutFile $env:TEMP/cometdesk-install.ps1; & $env:TEMP/cometdesk-install.ps1 {code}\""
        )),
        TargetOs::MacOs | TargetOs::Bios => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_commands_carry_the_code_and_fit_the_run_dialog() {
        let code = cometdesk_clip::Secret::generate().to_code();
        let linux = install_command(TargetOs::Linux, &code).unwrap();
        assert!(linux.ends_with(&format!("sh /tmp/cometdesk-install.sh {code}")));
        let win = install_command(TargetOs::Windows, &code).unwrap();
        assert!(win.contains(&code));
        // Typed over HID: no characters that need AltGr on common layouts.
        for cmd in [&linux, &win] {
            assert!(
                !cmd.contains(['|', '[', ']', '{', '}', '\\', '@', '~']),
                "{cmd}"
            );
        }
        // Windows' Run dialog accepts at most 259 characters.
        assert!(win.len() < 260, "{} chars", win.len());
        assert_eq!(install_command(TargetOs::MacOs, &code), None);
        assert_eq!(agent_name(TargetOs::Linux), Some("linux-x86_64"));
    }
}
