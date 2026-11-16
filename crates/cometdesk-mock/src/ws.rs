//! `/api/ws` of the mock device: 101 upgrade, initial state events, ping/pong, HID frame
//! capture and pushed events. Entry point: `serve`. Read this to change the event stream.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::Role;

use crate::http::Request;
use crate::{Push, State};

/// Undoes the viewer count when the socket ends, however it ends.
struct Viewer(Arc<State>);

impl Drop for Viewer {
    fn drop(&mut self) {
        self.0.viewers.fetch_sub(1, Ordering::SeqCst);
    }
}

fn event(kind: &str, json: &str) -> Message {
    let data: serde_json::Value = serde_json::from_str(json).unwrap();
    Message::text(serde_json::json!({"event_type": kind, "event": data}).to_string())
}

pub async fn serve<S: AsyncRead + AsyncWrite + Unpin>(state: Arc<State>, mut io: S, req: &Request) {
    let key = req.header("sec-websocket-key").unwrap_or_default();
    let head = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        derive_accept_key(key.as_bytes())
    );
    state.ws_sessions.fetch_add(1, Ordering::SeqCst);
    let streams =
        !url::form_urlencoded::parse(req.query.as_bytes()).any(|(k, v)| k == "stream" && v == "0");
    let _viewer = streams.then(|| {
        state.viewers.fetch_add(1, Ordering::SeqCst);
        Viewer(state.clone())
    });
    // Subscribe before the 101 so a push right after the client connects is not lost.
    let mut push = state.push.subscribe();
    if io.write_all(head.as_bytes()).await.is_err() {
        return;
    }
    let mut ws = WebSocketStream::from_raw_socket(io, Role::Server, None).await;
    let initial = [
        ("hid", crate::fixtures::result("hid")),
        ("atx", crate::routes::atx(&state)),
        ("streamer", crate::stateful::streamer(&state)),
    ];
    for (kind, json) in &initial {
        if ws.send(event(kind, json)).await.is_err() {
            return;
        }
    }
    loop {
        tokio::select! {
            msg = ws.next() => match msg {
                Some(Ok(Message::Text(t))) if t.contains(r#""event_type":"ping""#) => {
                    if ws.send(Message::text(r#"{"event_type":"pong","event":{}}"#)).await.is_err() {
                        return;
                    }
                }
                Some(Ok(Message::Binary(b))) => state.hid_frames.lock().unwrap().push(b.to_vec()),
                Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                Some(Ok(_)) => {}
            },
            p = push.recv() => match p {
                Ok(Push::Text(t)) => {
                    if ws.send(Message::text(t)).await.is_err() {
                        return;
                    }
                }
                Ok(Push::Close) => {
                    let _ = ws.close(None).await;
                    return;
                }
                Err(_) => {}
            },
        }
    }
}
