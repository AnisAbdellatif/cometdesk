//! Types shared by every video source (Direct, Pion, Janus) and the transport switch.

use std::future::Future;
use std::str::FromStr;

use glkvm::Session;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use crate::direct::{self, DirectOptions};
use crate::{Codec, EncodedFrame, janus, pion};

/// Frames buffered towards the decoder. In steady state the queue is empty (decode takes
/// ~2 ms per frame), so depth adds no latency; it only absorbs hiccups such as creating the
/// first VideoToolbox session (~70 ms), which with a shallow queue cost up to a whole GOP of
/// frozen video while waiting for the next keyframe.
pub(crate) const QUEUE_DEPTH: usize = 16;

/// How video reaches us. Mirrors the device's modes: Direct = `/api/media/ws`,
/// Pion = WebRTC (FEC) at `/pion/ws`, Janus = classic WebRTC at `/janus/ws`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Transport {
    #[default]
    Direct,
    Pion,
    Janus,
}

impl Transport {
    pub const ALL: [Transport; 3] = [Transport::Direct, Transport::Pion, Transport::Janus];

    pub fn name(self) -> &'static str {
        match self {
            Transport::Direct => "direct",
            Transport::Pion => "webrtc-fec",
            Transport::Janus => "webrtc",
        }
    }
}

impl FromStr for Transport {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "direct" => Ok(Transport::Direct),
            "pion" | "webrtc-fec" | "webrtc+fec" | "fec" => Ok(Transport::Pion),
            "janus" | "webrtc" => Ok(Transport::Janus),
            other => Err(format!("unknown transport `{other}` (direct, pion, janus)")),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SourceOptions {
    pub transport: Transport,
    /// Preferred codec. WebRTC sources only carry H.264.
    pub prefer: Codec,
}

impl Default for SourceOptions {
    fn default() -> Self {
        Self {
            transport: Transport::Direct,
            prefer: Codec::H264,
        }
    }
}

/// Starts the chosen source on the current tokio runtime.
pub fn start(session: Session, opts: SourceOptions) -> MediaSession {
    match opts.transport {
        Transport::Direct => direct::start(
            session,
            DirectOptions {
                prefer: opts.prefer,
            },
        ),
        Transport::Pion => pion::start(session),
        Transport::Janus => janus::start(session),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceState {
    Connecting,
    Streaming {
        codec: Codec,
        profile: Option<String>,
    },
    Stalled,
    Disconnected(String),
    Closed,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MediaStats {
    pub fps: f32,
    pub kbps: f32,
    pub frames_total: u64,
    pub dropped_total: u64,
    /// Times packets were lost for good (WebRTC sources): the picture may show artifacts
    /// until the next keyframe.
    pub gaps_total: u64,
}

pub(crate) enum Control {
    KeyFrame,
}

#[derive(Clone)]
pub struct MediaControl {
    pub(crate) tx: mpsc::UnboundedSender<Control>,
}

impl MediaControl {
    /// Ask the device for a keyframe (after a decode error or a dropped frame).
    pub fn request_keyframe(&self) {
        let _ = self.tx.send(Control::KeyFrame);
    }
}

pub struct MediaSession {
    /// Encoded frames. Replaced by a closed channel after [`MediaSession::take_video`].
    pub video: mpsc::Receiver<EncodedFrame>,
    pub control: MediaControl,
    pub stats: watch::Receiver<MediaStats>,
    pub state: watch::Receiver<SourceState>,
    task: JoinHandle<()>,
}

/// The sending halves a source task needs.
pub(crate) struct Ends {
    pub frames: mpsc::Sender<EncodedFrame>,
    pub ctl: mpsc::UnboundedReceiver<Control>,
    pub stats: watch::Sender<MediaStats>,
    pub state: watch::Sender<SourceState>,
}

impl MediaSession {
    /// Spawns `task` with fresh channels and returns the consumer side.
    pub(crate) fn spawn<F, Fut>(task: F) -> MediaSession
    where
        F: FnOnce(Ends) -> Fut,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let (frame_tx, video) = mpsc::channel(QUEUE_DEPTH);
        let (ctl_tx, ctl_rx) = mpsc::unbounded_channel();
        let (stats_tx, stats) = watch::channel(MediaStats::default());
        let (state_tx, state) = watch::channel(SourceState::Connecting);
        let task = tokio::spawn(task(Ends {
            frames: frame_tx,
            ctl: ctl_rx,
            stats: stats_tx,
            state: state_tx,
        }));
        MediaSession {
            video,
            control: MediaControl { tx: ctl_tx },
            stats,
            state,
            task,
        }
    }

    /// Moves the frame receiver out (e.g. to a decode thread), keeping the session alive.
    pub fn take_video(mut self) -> (mpsc::Receiver<EncodedFrame>, MediaSession) {
        let (_, closed) = mpsc::channel(1);
        let video = std::mem::replace(&mut self.video, closed);
        (video, self)
    }
}

impl Drop for MediaSession {
    fn drop(&mut self) {
        self.task.abort();
    }
}
