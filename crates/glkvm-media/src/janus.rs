//! "WebRTC" video: the classic PiKVM Janus + uStreamer plugin at `wss://host/janus/ws`.
//!
//! Needs the device in a mode where Janus runs: with `gl_webrtc=true` the endpoint answers
//! 502 (see `docs/protocol.md`). We never change that device-wide setting.
//!
//! Sequence (Janus is the offerer): `create` → `attach janus.plugin.ustreamer` →
//! `message {request: watch}` → event with a `jsep` offer → `message {request: start}` with
//! our answer → trickle candidates (`completed` at the end). Keepalive every 25 s.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use glkvm::{Error, Result, Session};
use serde_json::{Value, json};
use str0m::change::SdpOffer;
use str0m::media::MediaKind;
use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval, timeout};
use tokio_tungstenite::tungstenite::Message;

use crate::audio::AudioSession;
use crate::rtc::{self, AbortOnDrop, Negotiated, Peer, proto};
use crate::source::MediaSession;

const PROTOCOL: &str = "janus-protocol";
const PLUGIN: &str = "janus.plugin.ustreamer";
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);
const KEEPALIVE: Duration = Duration::from_secs(25);

/// Starts the Janus source on the current tokio runtime.
pub fn start(session: Session) -> MediaSession {
    MediaSession::spawn(|ends| {
        rtc::supervise("janus", ends, move || {
            negotiate(session.clone(), MediaKind::Video)
        })
    })
}

/// Starts the audio-only source: its own Janus handle and peer connection, as the web app
/// does in this mode.
pub fn start_audio(session: Session) -> AudioSession {
    AudioSession::spawn(|packets, state| {
        rtc::supervise_audio("janus", packets, state, move || {
            negotiate(session.clone(), MediaKind::Audio)
        })
    })
}

/// A Janus message we care about.
#[derive(Debug, PartialEq)]
pub enum Reply {
    /// `success` with `data.id` (create / attach).
    Id(u64),
    /// An `event` carrying a `jsep` offer.
    Offer(String),
    /// A trickled remote candidate.
    Candidate(String),
    /// `error` (Janus level) or a plugin error inside an `event`.
    Error(String),
    Other,
}

pub fn parse_reply(text: &str) -> Reply {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return Reply::Other;
    };
    match v["janus"].as_str() {
        Some("success") => v["data"]["id"].as_u64().map_or(Reply::Other, Reply::Id),
        Some("error") => Reply::Error(
            v["error"]["reason"]
                .as_str()
                .unwrap_or("unknown janus error")
                .to_owned(),
        ),
        Some("event") => {
            if v["jsep"]["type"].as_str() == Some("offer")
                && let Some(sdp) = v["jsep"]["sdp"].as_str()
            {
                return Reply::Offer(sdp.to_owned());
            }
            match v["plugindata"]["data"]["error"].as_str() {
                Some(e) => Reply::Error(e.to_owned()),
                None => Reply::Other,
            }
        }
        Some("trickle") => v["candidate"]["candidate"]
            .as_str()
            .map_or(Reply::Other, |c| Reply::Candidate(c.to_owned())),
        _ => Reply::Other,
    }
}

struct Tx(u32);

impl Tx {
    fn next(&mut self) -> String {
        self.0 += 1;
        format!("glkvm-{}", self.0)
    }
}

fn msg(v: Value) -> Message {
    Message::text(v.to_string())
}

type Ws = glkvm::client::WsStream;

/// Reads replies until `pick` accepts one.
async fn wait_for<T>(
    ws: &mut Ws,
    early: &mut Vec<String>,
    what: &str,
    mut pick: impl FnMut(Reply) -> Option<T>,
) -> Result<T> {
    timeout(REPLY_TIMEOUT, async {
        while let Some(m) = ws.next().await {
            let Message::Text(text) = m? else { continue };
            match parse_reply(&text) {
                Reply::Error(e) => return Err(Error::Protocol(format!("janus: {e}"))),
                Reply::Candidate(c) => early.push(c),
                other => {
                    if let Some(t) = pick(other) {
                        return Ok(t);
                    }
                }
            }
        }
        Err(Error::Protocol(format!(
            "janus socket ended waiting for {what}"
        )))
    })
    .await
    .map_err(|_| Error::Protocol(format!("janus: no {what} (is gl_webrtc enabled?)")))?
}

async fn negotiate(session: Session, kind: MediaKind) -> Result<Negotiated> {
    let video = kind == MediaKind::Video;
    let mut ws = session
        .ws_connect_protocol("janus/ws", Some(PROTOCOL))
        .await?;
    let mut tx = Tx(0);
    let mut early = Vec::new();

    ws.send(msg(json!({"janus": "create", "transaction": tx.next()})))
        .await?;
    let session_id = wait_for(&mut ws, &mut early, "session", |r| match r {
        Reply::Id(id) => Some(id),
        _ => None,
    })
    .await?;
    ws.send(msg(json!({
        "janus": "attach", "session_id": session_id, "plugin": PLUGIN,
        "transaction": tx.next(),
    })))
    .await?;
    let handle_id = wait_for(&mut ws, &mut early, "plugin handle", |r| match r {
        Reply::Id(id) => Some(id),
        _ => None,
    })
    .await?;
    // The device rotates the picture for WebRTC; a failed read just means no rotation.
    let orientation = session
        .system_config()
        .await
        .ok()
        .and_then(|c| c.orientation)
        .unwrap_or(0);
    ws.send(msg(json!({
        "janus": "message", "session_id": session_id, "handle_id": handle_id,
        "transaction": tx.next(),
        "body": {"request": "watch", "params": {
            "orientation": orientation, "audio": !video, "video": video, "mic": false, "camera": false,
        }},
    })))
    .await?;
    let offer = wait_for(&mut ws, &mut early, "offer", |r| match r {
        Reply::Offer(sdp) => Some(sdp),
        _ => None,
    })
    .await?;

    let mut peer = Peer::new(session.host(), rtc::REORDERING_PLAIN).await?;
    let offer = SdpOffer::from_sdp_string(&offer).map_err(proto)?;
    let answer = peer.rtc.sdp_api().accept_offer(offer).map_err(proto)?;
    ws.send(msg(json!({
        "janus": "message", "session_id": session_id, "handle_id": handle_id,
        "transaction": tx.next(),
        "body": {"request": "start"},
        "jsep": {"type": "answer", "sdp": answer.to_sdp_string()},
    })))
    .await?;
    ws.send(msg(json!({
        "janus": "trickle", "session_id": session_id, "handle_id": handle_id,
        "transaction": tx.next(), "candidate": {"completed": true},
    })))
    .await?;

    let (cand_tx, remote) = mpsc::unbounded_channel();
    for c in early {
        if let Some(c) = rtc::parse_candidate(&c) {
            let _ = cand_tx.send(c);
        }
    }
    let task = tokio::spawn(async move {
        let mut keepalive = interval(KEEPALIVE);
        keepalive.set_missed_tick_behavior(MissedTickBehavior::Delay);
        keepalive.tick().await;
        loop {
            tokio::select! {
                _ = keepalive.tick() => {
                    let m = msg(json!({
                        "janus": "keepalive", "session_id": session_id,
                        "transaction": tx.next(),
                    }));
                    if ws.send(m).await.is_err() { return; }
                }
                m = ws.next() => match m {
                    Some(Ok(Message::Text(text))) => {
                        tracing::debug!(%text, "janus message");
                        if let Reply::Candidate(c) = parse_reply(&text)
                            && let Some(c) = rtc::parse_candidate(&c) {
                                let _ = cand_tx.send(c);
                            }
                    }
                    Some(Ok(_)) => {}
                    _ => return,
                },
            }
        }
    });
    Ok(Negotiated {
        peer,
        remote,
        _signaling: AbortOnDrop(task),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies() {
        assert_eq!(
            parse_reply(r#"{"janus":"success","transaction":"a","data":{"id":42}}"#),
            Reply::Id(42)
        );
        assert_eq!(
            parse_reply(r#"{"janus":"event","jsep":{"type":"offer","sdp":"v=0"}}"#),
            Reply::Offer("v=0".into())
        );
        assert_eq!(
            parse_reply(r#"{"janus":"error","error":{"code":458,"reason":"No such session"}}"#),
            Reply::Error("No such session".into())
        );
        assert_eq!(
            parse_reply(r#"{"janus":"event","plugindata":{"data":{"error":"busy"}}}"#),
            Reply::Error("busy".into())
        );
        assert_eq!(
            parse_reply(
                r#"{"janus":"trickle","candidate":{"candidate":"candidate:1 1 udp 1 1.2.3.4 5 typ host"}}"#
            ),
            Reply::Candidate("candidate:1 1 udp 1 1.2.3.4 5 typ host".into())
        );
        assert_eq!(parse_reply(r#"{"janus":"ack"}"#), Reply::Other);
        assert_eq!(
            parse_reply(r#"{"janus":"trickle","candidate":{"completed":true}}"#),
            Reply::Other
        );
    }
}
