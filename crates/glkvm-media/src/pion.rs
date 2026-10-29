//! "WebRTC (FEC)" video: GL's Pion server at `wss://host/pion/ws`.
//!
//! We are the offerer. Signaling, all JSON text frames:
//! - client → `{"type":"config","video":true,"speaker":false,"mic":false,"hid":false,
//!   "offerer":true,"flexfecLevel":0,"orientation":0,"camera":false}`;
//! - client → `{"type":"offer","sdp":"…"}`, server → `{"type":"answer","sdp":"…"}`;
//! - either side → `{"type":"candidate","candidate":"candidate:…"}`.
//!
//! The field layout of the replies is lenient on purpose (see [`parse_signal`]); it has not
//! been confirmed against a device yet. We offer plain RTP + NACK, no FlexFEC (str0m has
//! none); whether gl-pion accepts that is what `glkvm-cli view --transport pion` shows.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use glkvm::{Error, Result, Session};
use serde_json::{Value, json};
use str0m::change::SdpAnswer;
use str0m::media::{Direction, MediaKind};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;

use crate::audio::AudioSession;
use crate::rtc::{self, AbortOnDrop, Negotiated, Peer, proto};
use crate::source::MediaSession;

const SIGNALING_TIMEOUT: Duration = Duration::from_secs(10);

/// Starts the Pion source on the current tokio runtime.
pub fn start(session: Session) -> MediaSession {
    MediaSession::spawn(|ends| {
        rtc::supervise("pion", ends, move || {
            negotiate(session.clone(), MediaKind::Video)
        })
    })
}

/// Starts the audio-only source: a second connection next to the video one, because sharing
/// a peer connection makes the receiver hold video back for A/V sync.
pub fn start_audio(session: Session) -> AudioSession {
    AudioSession::spawn(|packets, state| {
        rtc::supervise_audio("pion", packets, state, move || {
            negotiate(session.clone(), MediaKind::Audio)
        })
    })
}

#[derive(Debug, PartialEq)]
pub enum Signal {
    Answer(String),
    Candidate(String),
    Other,
}

/// Classifies a signaling message. The SDP may sit at `sdp`, `data.sdp` or `answer.sdp`; a
/// candidate is a string or an object with a `candidate` field.
pub fn parse_signal(text: &str) -> Signal {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return Signal::Other;
    };
    match v.get("type").and_then(Value::as_str) {
        Some("answer") => [
            &v["sdp"],
            &v["data"]["sdp"],
            &v["answer"]["sdp"],
            &v["data"],
        ]
        .into_iter()
        .find_map(Value::as_str)
        .map_or(Signal::Other, |s| Signal::Answer(s.to_owned())),
        Some("candidate") => [&v["candidate"], &v["candidate"]["candidate"], &v["data"]]
            .into_iter()
            .find_map(Value::as_str)
            .filter(|s| !s.is_empty())
            .map_or(Signal::Other, |s| Signal::Candidate(s.to_owned())),
        _ => Signal::Other,
    }
}

fn config_message(kind: MediaKind) -> Message {
    let video = kind == MediaKind::Video;
    Message::text(
        json!({
            "type": "config",
            "video": video,
            "speaker": !video,
            "mic": false,
            "hid": false,
            "offerer": true,
            "flexfecLevel": 0,
            "orientation": 0,
            "camera": false,
        })
        .to_string(),
    )
}

async fn negotiate(session: Session, kind: MediaKind) -> Result<Negotiated> {
    let ws = session.ws_connect("pion/ws").await?;
    let (mut sink, mut stream) = ws.split();
    let mut peer = Peer::new(session.host()).await?;

    let mut api = peer.rtc.sdp_api();
    api.add_media(kind, Direction::RecvOnly, None, None, None);
    let (offer, pending) = api
        .apply()
        .ok_or_else(|| Error::Protocol("no offer produced".into()))?;
    tracing::debug!(sdp = %offer.to_sdp_string(), "pion offer");
    sink.send(config_message(kind)).await?;
    sink.send(Message::text(
        json!({"type": "offer", "sdp": offer.to_sdp_string()}).to_string(),
    ))
    .await?;

    let (cand_tx, remote) = mpsc::unbounded_channel();
    // Candidates may arrive before the answer; hold them until the answer is applied.
    let mut early = Vec::new();
    let answer = timeout(SIGNALING_TIMEOUT, async {
        while let Some(msg) = stream.next().await {
            let Message::Text(text) = msg? else { continue };
            tracing::debug!(%text, "pion signaling");
            match parse_signal(&text) {
                Signal::Answer(sdp) => return Ok(sdp),
                Signal::Candidate(c) => early.push(c),
                Signal::Other => {}
            }
        }
        Err(Error::Protocol(
            "pion socket ended before the answer".into(),
        ))
    })
    .await
    .map_err(|_| Error::Protocol("no answer from pion".into()))??;
    tracing::info!(early_candidates = early.len(), "pion answer received");
    tracing::debug!(sdp = %answer, "pion answer");
    let answer = SdpAnswer::from_sdp_string(&answer).map_err(proto)?;
    peer.rtc
        .sdp_api()
        .accept_answer(pending, answer)
        .map_err(proto)?;
    for c in early {
        if let Some(c) = rtc::parse_candidate(&c) {
            let _ = cand_tx.send(c);
        }
    }

    let task = tokio::spawn(async move {
        let _sink = sink; // keeps the socket open
        while let Some(Ok(msg)) = stream.next().await {
            if let Message::Text(text) = msg
                && let Signal::Candidate(c) = parse_signal(&text)
                && let Some(c) = rtc::parse_candidate(&c)
            {
                let _ = cand_tx.send(c);
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
    fn answer_in_any_layout() {
        for t in [
            r#"{"type":"answer","sdp":"v=0"}"#,
            r#"{"type":"answer","data":{"sdp":"v=0"}}"#,
            r#"{"type":"answer","answer":{"sdp":"v=0"}}"#,
        ] {
            assert_eq!(parse_signal(t), Signal::Answer("v=0".into()), "{t}");
        }
    }

    #[test]
    fn candidate_string_or_object() {
        let c = "candidate:1 1 udp 2130706431 192.168.8.174 5000 typ host";
        for t in [
            json!({"type":"candidate","candidate":c}),
            json!({"type":"candidate","candidate":{"candidate":c,"sdpMid":"0"}}),
        ] {
            assert_eq!(parse_signal(&t.to_string()), Signal::Candidate(c.into()));
        }
        assert_eq!(
            parse_signal(r#"{"type":"candidate","candidate":""}"#),
            Signal::Other
        );
        assert!(rtc::parse_candidate(c).is_some());
        assert!(rtc::parse_candidate(&format!("a={c}")).is_some());
    }

    #[test]
    fn ignores_noise() {
        assert_eq!(parse_signal("not json"), Signal::Other);
        assert_eq!(parse_signal(r#"{"type":"pong"}"#), Signal::Other);
    }
}
