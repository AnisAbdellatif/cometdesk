//! Internal console state types: media handles, in-flight tasks, modals and drawer tabs.
//! Read this before changing `Console` fields, confirmation prompts or video settings state.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};

use cometdesk::api::atx::{AtxButton, PowerAction};
use cometdesk::api::edid::EdidPreset;
use cometdesk::api::streamer::{StreamerLimits, StreamerParams, StreamerUpdate};
use cometdesk::api::system::{LatencyMode, Orientation, VencMode, VideoFormat, VideoMode};
use cometdesk_clip::PowerAction as SessionAction;
use cometdesk_media::audio::{AudioSession, Player};
use cometdesk_media::direct::MediaSession;
use cometdesk_view::pipeline::FramePipeline;
use tokio::task::JoinHandle;

pub(super) struct Stream {
    pub(super) media: MediaSession,
    pub(super) pipeline: FramePipeline,
    pub(super) audio: Option<Audio>,
}

/// Target audio: its own WebRTC connection plus the output stream. Lives and dies with the
/// video stream, so a hidden window plays and fetches nothing.
pub(super) struct Audio {
    pub(super) _player: Player,
    pub(super) _session: AudioSession,
}

pub(super) struct Typing {
    pub(super) total: usize,
    pub(super) done: Arc<AtomicUsize>,
    pub(super) task: JoinHandle<Result<(), String>>,
}

/// A region being picked: the drag's start (once it began) and the pointer now.
pub(super) struct RegionPick {
    pub(super) start: Option<egui::Pos2>,
    pub(super) now: egui::Pos2,
}

pub(super) enum Confirm {
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
pub(super) enum VideoChange {
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
pub(super) struct VideoInfo {
    pub(super) codec: Option<VideoFormat>,
    pub(super) h265_supported: bool,
    pub(super) mode: Option<VideoMode>,
    pub(super) venc: Option<VencMode>,
    pub(super) latency: Option<LatencyMode>,
    pub(super) orientation: Orientation,
    /// Normalized hex of the EDID the target currently sees.
    pub(super) edid: String,
    pub(super) edid_presets: Vec<EdidPreset>,
    pub(super) params: StreamerParams,
    pub(super) limits: StreamerLimits,
}

#[derive(Default)]
pub(super) struct FrameStats {
    pub(super) window_start: Option<Instant>,
    pub(super) frames: u32,
    pub(super) net_to_screen: Duration,
    pub(super) decode: Duration,
    pub(super) shown: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Tab {
    Target,
    Video,
    Settings,
}

impl Tab {
    pub(super) const ALL: [Tab; 3] = [Tab::Target, Tab::Video, Tab::Settings];

    pub(super) fn label(self) -> &'static str {
        match self {
            Tab::Target => "Target",
            Tab::Video => "Video",
            Tab::Settings => "Settings",
        }
    }
}
