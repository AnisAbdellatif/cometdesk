//! "Direct" video: raw H.264/H.265 access units over `wss://host/api/media/ws`.
//!
//! Protocol (kvmd media server):
//! - on connect the device sends a JSON event `media` listing formats, e.g.
//!   `{"video": {"h264": {"profile_level_id": "42E01F"}}}`;
//! - the client sends `{"event_type": "start", "event": {"type": "video", "format": "h264"}}`;
//! - video arrives as binary `[0x01][key: 0|1][Annex B access unit]`;
//! - binary `[0x00]` is a ping (answered with `[0xFF]`), `[0x01]` requests a keyframe.
//!
//! The device only produces frames while its streamer runs, which needs an `/api/ws`
//! session with streaming enabled (see `glkvm::events`).
//!
//! There is no jitter buffer: frames are forwarded on arrival. If the consumer falls behind
//! the newest frames win: queued frames are dropped and a keyframe is requested.

use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use glkvm::Session;
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior, sleep, timeout};
use tokio_tungstenite::tungstenite::Message;

use crate::{Codec, EncodedFrame};

const OP_PING: u8 = 0;
const OP_FRAME: u8 = 1;
const OP_KEY_REQUEST: u8 = 1;
const OP_PONG: u8 = 255;

const PING_INTERVAL: Duration = Duration::from_secs(1);
const MAX_MISSED_PONGS: u32 = 5;
const MEDIA_EVENT_TIMEOUT: Duration = Duration::from_secs(5);
/// No frame for this long: ask for a keyframe.
const STALL_KEY_REQUEST: Duration = Duration::from_secs(3);
/// No frame for this long (or no first frame): reconnect.
const STALL_RECONNECT: Duration = Duration::from_secs(5);
const BACKOFF_MIN: Duration = Duration::from_millis(250);
const BACKOFF_MAX: Duration = Duration::from_secs(10);
/// Frames buffered towards the decoder. In steady state the queue is empty (decode takes
/// ~2 ms per frame), so depth adds no latency; it only absorbs hiccups such as creating the
/// first VideoToolbox session (~70 ms), which with a shallow queue cost up to a whole GOP of
/// frozen video while waiting for the next keyframe.
const QUEUE_DEPTH: usize = 16;

#[derive(Debug, Clone, Copy)]
pub struct DirectOptions {
    pub prefer: Codec,
}

impl Default for DirectOptions {
    fn default() -> Self {
        Self {
            prefer: Codec::H264,
        }
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
}

enum Control {
    KeyFrame,
}

#[derive(Clone)]
pub struct MediaControl {
    tx: mpsc::UnboundedSender<Control>,
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

impl MediaSession {
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

/// Starts the Direct source on the current tokio runtime.
pub fn start(session: Session, opts: DirectOptions) -> MediaSession {
    let (frame_tx, video) = mpsc::channel(QUEUE_DEPTH);
    let (ctl_tx, ctl_rx) = mpsc::unbounded_channel();
    let (stats_tx, stats) = watch::channel(MediaStats::default());
    let (state_tx, state) = watch::channel(SourceState::Connecting);
    let task = tokio::spawn(supervise(
        session, opts, frame_tx, ctl_rx, stats_tx, state_tx,
    ));
    MediaSession {
        video,
        control: MediaControl { tx: ctl_tx },
        stats,
        state,
        task,
    }
}

/// A parsed binary message from the media socket.
#[derive(Debug, PartialEq)]
pub enum BinMessage {
    Pong,
    Frame { key: bool, data: Bytes },
    Unknown(u8),
}

pub fn parse_bin(msg: Bytes) -> Option<BinMessage> {
    match *msg.first()? {
        OP_PONG => Some(BinMessage::Pong),
        OP_FRAME if msg.len() >= 2 => Some(BinMessage::Frame {
            key: msg[1] != 0,
            data: msg.slice(2..),
        }),
        OP_FRAME => None,
        op => Some(BinMessage::Unknown(op)),
    }
}

/// Picks a format from the `media` event: the preferred codec if offered, else the other.
pub fn choose_format(media: &Value, prefer: Codec) -> Option<(Codec, Option<String>)> {
    let video = media.get("video")?.as_object()?;
    let other = match prefer {
        Codec::H264 => Codec::H265,
        Codec::H265 => Codec::H264,
    };
    [prefer, other].into_iter().find_map(|codec| {
        let meta = video.get(codec.wire_name())?;
        let profile = meta
            .get("profile_level_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        Some((codec, profile))
    })
}

fn start_message(codec: Codec) -> Message {
    Message::text(
        serde_json::json!({
            "event_type": "start",
            "event": {"type": "video", "format": codec.wire_name()},
        })
        .to_string(),
    )
}

async fn supervise(
    session: Session,
    opts: DirectOptions,
    frame_tx: mpsc::Sender<EncodedFrame>,
    mut ctl_rx: mpsc::UnboundedReceiver<Control>,
    stats_tx: watch::Sender<MediaStats>,
    state_tx: watch::Sender<SourceState>,
) {
    let mut backoff = BACKOFF_MIN;
    let mut stats = MediaStats::default();
    loop {
        if frame_tx.is_closed() {
            state_tx.send_replace(SourceState::Closed);
            return;
        }
        state_tx.send_replace(SourceState::Connecting);
        let reason = match run(
            &session,
            opts,
            &frame_tx,
            &mut ctl_rx,
            &stats_tx,
            &state_tx,
            &mut stats,
        )
        .await
        {
            Ok(Streamed::Yes(reason)) => {
                backoff = BACKOFF_MIN;
                reason
            }
            Ok(Streamed::No(reason)) => reason,
            Err(e) => e.to_string(),
        };
        if frame_tx.is_closed() {
            state_tx.send_replace(SourceState::Closed);
            return;
        }
        tracing::warn!(%reason, retry_in = ?backoff, "media socket lost");
        state_tx.send_replace(SourceState::Disconnected(reason));
        sleep(backoff).await;
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

enum Streamed {
    /// At least one frame arrived before the connection ended.
    Yes(String),
    No(String),
}

async fn run(
    session: &Session,
    opts: DirectOptions,
    frame_tx: &mpsc::Sender<EncodedFrame>,
    ctl_rx: &mut mpsc::UnboundedReceiver<Control>,
    stats_tx: &watch::Sender<MediaStats>,
    state_tx: &watch::Sender<SourceState>,
    stats: &mut MediaStats,
) -> glkvm::Result<Streamed> {
    let ws = session.ws_connect("api/media/ws").await?;
    let (mut sink, mut stream) = ws.split();

    // 1. Wait for the `media` event and pick a format.
    let (codec, profile) = timeout(MEDIA_EVENT_TIMEOUT, async {
        while let Some(msg) = stream.next().await {
            if let Message::Text(text) = msg? {
                let v: Value = serde_json::from_str(&text)?;
                if v.get("event_type").and_then(Value::as_str) == Some("media") {
                    return choose_format(&v["event"], opts.prefer).ok_or_else(|| {
                        glkvm::Error::Protocol(format!(
                            "no H.264/H.265 stream offered: {}",
                            v["event"]
                        ))
                    });
                }
            }
        }
        Err(glkvm::Error::Protocol(
            "socket ended before media event".into(),
        ))
    })
    .await
    .map_err(|_| glkvm::Error::Protocol("no media event from device".into()))??;
    tracing::info!(?codec, ?profile, "direct stream offered");

    sink.send(start_message(codec)).await?;
    state_tx.send_replace(SourceState::Streaming {
        codec,
        profile: profile.clone(),
    });

    // 2. Stream.
    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut missed_pongs = 0u32;
    let connected_at = Instant::now();
    let mut last_frame: Option<Instant> = None;
    let mut key_requested_for_stall = false;
    let mut waiting_for_key = false;
    let mut window_start = Instant::now();
    let (mut window_frames, mut window_bytes) = (0u32, 0usize);
    let mut got_any = false;

    let ended = |got_any: bool, reason: String| {
        if got_any {
            Streamed::Yes(reason)
        } else {
            Streamed::No(reason)
        }
    };

    loop {
        tokio::select! {
            biased;
            msg = stream.next() => {
                let msg = match msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return Ok(ended(got_any, e.to_string())),
                    None => return Ok(ended(got_any, "socket ended".into())),
                };
                let Message::Binary(bin) = msg else {
                    if let Message::Close(f) = msg {
                        return Ok(ended(got_any, format!("closed by device: {f:?}")));
                    }
                    continue;
                };
                match parse_bin(bin) {
                    Some(BinMessage::Pong) => missed_pongs = 0,
                    Some(BinMessage::Frame { key, data }) => {
                        let now = Instant::now();
                        got_any = true;
                        last_frame = Some(now);
                        if key_requested_for_stall {
                            key_requested_for_stall = false;
                            state_tx.send_replace(SourceState::Streaming { codec, profile: profile.clone() });
                        }
                        stats.frames_total += 1;
                        window_frames += 1;
                        window_bytes += data.len();
                        if waiting_for_key && !key {
                            stats.dropped_total += 1;
                            continue;
                        }
                        let frame = EncodedFrame { codec, data, keyframe: key, received_at: now.into_std() };
                        match frame_tx.try_send(frame) {
                            Ok(()) => waiting_for_key = false,
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                // Consumer is behind: skip to the next keyframe.
                                stats.dropped_total += 1;
                                waiting_for_key = true;
                                sink.send(Message::binary(vec![OP_KEY_REQUEST])).await?;
                            }
                            Err(mpsc::error::TrySendError::Closed(_)) => {
                                let _ = sink.close().await;
                                return Ok(ended(got_any, "consumer closed".into()));
                            }
                        }
                    }
                    Some(BinMessage::Unknown(op)) => tracing::debug!(op, "unknown media op"),
                    None => {}
                }
            }
            ctl = ctl_rx.recv() => {
                if let Some(Control::KeyFrame) = ctl {
                    sink.send(Message::binary(vec![OP_KEY_REQUEST])).await?;
                }
            }
            _ = ping.tick() => {
                missed_pongs += 1;
                if missed_pongs > MAX_MISSED_PONGS {
                    return Ok(ended(got_any, "no pong from media server".into()));
                }
                sink.send(Message::binary(vec![OP_PING])).await?;

                let now = Instant::now();
                let since = now - last_frame.unwrap_or(connected_at);
                if since >= STALL_RECONNECT {
                    return Ok(ended(got_any, format!("no video for {since:?}")));
                }
                if since >= STALL_KEY_REQUEST && !key_requested_for_stall {
                    key_requested_for_stall = true;
                    state_tx.send_replace(SourceState::Stalled);
                    sink.send(Message::binary(vec![OP_KEY_REQUEST])).await?;
                }

                let secs = (now - window_start).as_secs_f32();
                if secs >= 1.0 {
                    stats.fps = window_frames as f32 / secs;
                    stats.kbps = window_bytes as f32 * 8.0 / 1000.0 / secs;
                    stats_tx.send_replace(*stats);
                    window_start = now;
                    window_frames = 0;
                    window_bytes = 0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_frames_and_pong() {
        assert_eq!(
            parse_bin(Bytes::from_static(&[255])),
            Some(BinMessage::Pong)
        );
        assert_eq!(
            parse_bin(Bytes::from_static(&[1, 1, 0, 0, 1, 0x65])),
            Some(BinMessage::Frame {
                key: true,
                data: Bytes::from_static(&[0, 0, 1, 0x65])
            })
        );
        assert_eq!(
            parse_bin(Bytes::from_static(&[1, 0, 9])),
            Some(BinMessage::Frame {
                key: false,
                data: Bytes::from_static(&[9])
            })
        );
        assert_eq!(parse_bin(Bytes::from_static(&[1])), None);
        assert_eq!(parse_bin(Bytes::new()), None);
        assert_eq!(
            parse_bin(Bytes::from_static(&[7, 0])),
            Some(BinMessage::Unknown(7))
        );
    }

    #[test]
    fn chooses_preferred_codec() {
        let media =
            json!({"video": {"h264": {"profile_level_id": "42E01F"}, "h265": {}, "jpeg": {}}});
        assert_eq!(
            choose_format(&media, Codec::H264),
            Some((Codec::H264, Some("42E01F".into())))
        );
        assert_eq!(
            choose_format(&media, Codec::H265),
            Some((Codec::H265, None))
        );
    }

    #[test]
    fn falls_back_to_other_codec_and_ignores_jpeg() {
        let only265 = json!({"video": {"h265": {"profile_level_id": "x"}}});
        assert_eq!(
            choose_format(&only265, Codec::H264),
            Some((Codec::H265, Some("x".into())))
        );
        let only_jpeg = json!({"video": {"jpeg": {}}});
        assert_eq!(choose_format(&only_jpeg, Codec::H264), None);
        assert_eq!(choose_format(&json!({}), Codec::H264), None);
    }

    #[test]
    fn start_message_shape() {
        let Message::Text(t) = start_message(Codec::H264) else {
            panic!()
        };
        let v: Value = serde_json::from_str(&t).unwrap();
        assert_eq!(
            v,
            json!({"event_type": "start", "event": {"type": "video", "format": "h264"}})
        );
    }
}
