//! Types shared by every video source (Direct, Pion, Janus) and the transport switch.

use std::future::Future;
use std::str::FromStr;
use std::time::Instant;

use cometdesk::Session;
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

    /// The transport that can actually carry `codec`: gl-pion sends no video at all while
    /// the encoder is on H.265 (firmware V1.10.1), so H.265 over Pion falls back to Direct.
    pub fn for_codec(self, codec: Codec) -> Transport {
        match (self, codec) {
            (Transport::Pion, Codec::H265) => Transport::Direct,
            _ => self,
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
    let transport = opts.transport.for_codec(opts.prefer);
    if transport != opts.transport {
        tracing::warn!(
            requested = opts.transport.name(),
            used = transport.name(),
            "gl-pion does not stream H.265"
        );
    }
    match transport {
        Transport::Direct => direct::start(
            session,
            DirectOptions {
                prefer: opts.prefer,
            },
        ),
        Transport::Pion => pion::start(session, opts.prefer),
        Transport::Janus => janus::start(session, opts.prefer),
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
    pub codec: Option<Codec>,
    pub fps: f32,
    pub kbps: f32,
    pub frames_total: u64,
    pub dropped_total: u64,
    /// Times packets were lost for good (WebRTC sources): the picture may show artifacts
    /// until the next keyframe.
    pub gaps_total: u64,
    /// Average keyframe size in bytes from the last window that had one (0 before any).
    pub avg_keyframe_bytes: usize,
    /// Average delta-frame size in bytes from the last window that had one (0 before any).
    pub avg_delta_bytes: usize,
    /// Total keyframes received.
    pub keyframes_total: u64,
    /// Total bytes in keyframes received.
    pub keyframe_bytes_total: u64,
    /// Total delta frames received.
    pub delta_frames_total: u64,
    /// Total bytes in delta frames received.
    pub delta_bytes_total: u64,
}

/// Helper for transport loops (Direct, Pion, Janus) to track windowed frame stats.
#[derive(Debug, Clone)]
pub struct FrameTracker {
    window_start: Instant,
    key_frames: u32,
    key_bytes: usize,
    delta_frames: u32,
    delta_bytes: usize,
}

impl Default for FrameTracker {
    fn default() -> Self {
        Self {
            window_start: Instant::now(),
            key_frames: 0,
            key_bytes: 0,
            delta_frames: 0,
            delta_bytes: 0,
        }
    }
}

impl FrameTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one incoming frame into `stats` totals and window accumulators.
    pub fn record_frame(&mut self, stats: &mut MediaStats, keyframe: bool, len: usize) {
        stats.frames_total += 1;
        if keyframe {
            stats.keyframes_total += 1;
            stats.keyframe_bytes_total += len as u64;
            self.key_frames += 1;
            self.key_bytes += len;
        } else {
            stats.delta_frames_total += 1;
            stats.delta_bytes_total += len as u64;
            self.delta_frames += 1;
            self.delta_bytes += len;
        }
    }

    /// If at least 1 second has elapsed since window start, updates `stats` window rates and returns true.
    pub fn tick(&mut self, stats: &mut MediaStats, now: Instant) -> bool {
        let elapsed = now.duration_since(self.window_start);
        if elapsed.as_secs_f32() < 1.0 {
            return false;
        }
        let secs = elapsed.as_secs_f32();
        let frames = self.key_frames + self.delta_frames;
        stats.fps = frames as f32 / secs;
        stats.kbps = (self.key_bytes + self.delta_bytes) as f32 * 8.0 / 1000.0 / secs;
        // Keyframes are rare: a window without one keeps the last average.
        if let Some(avg) = self.key_bytes.checked_div(self.key_frames as usize) {
            stats.avg_keyframe_bytes = avg;
        }
        if let Some(avg) = self.delta_bytes.checked_div(self.delta_frames as usize) {
            stats.avg_delta_bytes = avg;
        }
        self.window_start = now;
        self.key_frames = 0;
        self.key_bytes = 0;
        self.delta_frames = 0;
        self.delta_bytes = 0;
        true
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn h265_over_pion_falls_back_to_direct() {
        assert_eq!(Transport::Pion.for_codec(Codec::H265), Transport::Direct);
        assert_eq!(Transport::Pion.for_codec(Codec::H264), Transport::Pion);
        assert_eq!(Transport::Janus.for_codec(Codec::H265), Transport::Janus);
    }

    #[test]
    fn frame_tracker_and_media_stats() {
        let mut stats = MediaStats::default();
        let mut tracker = FrameTracker::new();
        let t0 = tracker.window_start;

        tracker.record_frame(&mut stats, true, 100_000);
        tracker.record_frame(&mut stats, false, 1_000);
        tracker.record_frame(&mut stats, false, 2_000);

        assert_eq!(stats.frames_total, 3);
        assert_eq!(stats.keyframes_total, 1);
        assert_eq!(stats.keyframe_bytes_total, 100_000);
        assert_eq!(stats.delta_frames_total, 2);
        assert_eq!(stats.delta_bytes_total, 3_000);

        // Before 1s elapsed, tick does not fire
        assert!(!tracker.tick(&mut stats, t0 + Duration::from_millis(500)));

        // After 1s elapsed
        assert!(tracker.tick(&mut stats, t0 + Duration::from_secs(1)));
        assert_eq!(stats.fps, 3.0);
        assert_eq!(stats.avg_keyframe_bytes, 100_000);
        assert_eq!(stats.avg_delta_bytes, 1_500);
    }
}
