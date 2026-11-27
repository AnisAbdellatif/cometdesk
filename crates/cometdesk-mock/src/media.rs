//! `/api/media/ws` of the mock device: direct H.264 video from an embedded clip, looped.
//! Entry point: `serve`. Protocol: docs/protocol.md U2. Read this to change the video stream.

use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::Role;

use crate::State;
use crate::http::Request;

static CLIP: &[u8] = include_bytes!("clip.h264");
const FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// The clip as `(is_keyframe, Annex B access unit)`, split at access unit delimiters.
fn access_units() -> &'static Vec<(bool, Vec<u8>)> {
    static UNITS: OnceLock<Vec<(bool, Vec<u8>)>> = OnceLock::new();
    UNITS.get_or_init(|| {
        let mut starts = Vec::new(); // (offset of start code, nal type)
        let mut i = 0;
        while i + 3 < CLIP.len() {
            if CLIP[i..i + 3] == [0, 0, 1] {
                let at = if i > 0 && CLIP[i - 1] == 0 { i - 1 } else { i };
                starts.push((at, CLIP[i + 3] & 0x1f));
                i += 3;
            } else {
                i += 1;
            }
        }
        let mut units: Vec<(bool, Vec<u8>)> = Vec::new();
        for (n, &(at, kind)) in starts.iter().enumerate() {
            let end = starts.get(n + 1).map_or(CLIP.len(), |s| s.0);
            if kind == 9 {
                units.push((false, Vec::new()));
            }
            if let Some(unit) = units.last_mut() {
                unit.0 |= kind == 5;
                unit.1.extend_from_slice(&CLIP[at..end]);
            }
        }
        units
    })
}

/// Serves one media socket until the client leaves. Frames flow only while a streaming
/// `/api/ws` session is open (U3).
pub async fn serve<S: AsyncRead + AsyncWrite + Unpin>(state: Arc<State>, mut io: S, req: &Request) {
    if crate::ws::handshake(&mut io, req).await.is_err() {
        return;
    }
    state.media_sessions.fetch_add(1, Ordering::SeqCst);
    let mut ws = WebSocketStream::from_raw_socket(io, Role::Server, None).await;
    let offer =
        r#"{"event_type":"media","event":{"video":{"h264":{"profile_level_id":"42E01F"}}}}"#;
    if ws.send(Message::text(offer)).await.is_err() {
        return;
    }
    let units = access_units();
    let (mut started, mut next) = (false, 0usize);
    let mut tick = tokio::time::interval(FRAME_INTERVAL);
    loop {
        tokio::select! {
            msg = ws.next() => match msg {
                Some(Ok(Message::Text(t))) => {
                    let v: serde_json::Value = serde_json::from_str(&t).unwrap_or_default();
                    if v["event_type"] == "start" {
                        started = v["event"]["type"] == "video" && v["event"]["format"] == "h264";
                        next = 0;
                    }
                }
                Some(Ok(Message::Binary(b))) => match b.first() {
                    Some(0x00) => {
                        if ws.send(Message::binary(vec![0xFF])).await.is_err() {
                            return;
                        }
                    }
                    Some(0x01) => next = 0, // keyframe request: restart at the first IDR
                    _ => {}
                },
                Some(Ok(Message::Close(_))) => {
                    let _ = ws.flush().await;
                    return;
                }
                Some(Err(_)) | None => return,
                Some(Ok(_)) => {}
            },
            _ = tick.tick() => {
                if started && state.viewers.load(Ordering::SeqCst) > 0 {
                    let (key, data) = &units[next];
                    let mut frame = Vec::with_capacity(data.len() + 2);
                    frame.extend_from_slice(&[0x01, u8::from(*key)]);
                    frame.extend_from_slice(data);
                    if ws.send(Message::binary(frame)).await.is_err() {
                        return;
                    }
                    state.media_frames.fetch_add(1, Ordering::SeqCst);
                    next = (next + 1) % units.len();
                }
            }
        }
    }
}
