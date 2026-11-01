//! WebRTC audio-only supervision and packet pump (`supervise_audio`, `pump_audio`).
//! Entry point: `supervise_audio`, spawned by `pion` and `janus` audio streams.
//! To change video WebRTC supervision or str0m peer setup, read `mod.rs`.

use std::future::Future;
use std::time::Instant;

use bytes::Bytes;
use cometdesk::Result;
use str0m::net::{Protocol, Receive};
use str0m::{Event, IceConnectionState, Input, Output};
use tokio::sync::{mpsc, watch};
use tokio::time::{sleep, sleep_until};

use super::{BACKOFF_MAX, BACKOFF_MIN, Negotiated, Peer};
use crate::audio::AudioState;

/// Reconnect loop for an audio-only connection: Opus payloads out, nothing in. There is no
/// stall watchdog, because a silent target may legitimately send nothing (DTX); loss of the
/// connection itself is detected by ICE.
pub(crate) async fn supervise_audio<F, Fut>(
    name: &'static str,
    packets: mpsc::Sender<Bytes>,
    state: watch::Sender<AudioState>,
    connect: F,
) where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<Negotiated>>,
{
    let mut backoff = BACKOFF_MIN;
    loop {
        if packets.is_closed() {
            state.send_replace(AudioState::Closed);
            return;
        }
        state.send_replace(AudioState::Connecting);
        let reason = match connect().await {
            Ok(neg) => {
                let (reason, streamed) = pump_audio(neg, &packets, &state).await;
                if streamed {
                    backoff = BACKOFF_MIN;
                }
                reason
            }
            Err(e) => e.to_string(),
        };
        if packets.is_closed() {
            state.send_replace(AudioState::Closed);
            return;
        }
        tracing::warn!(%reason, retry_in = ?backoff, "{name} audio connection lost");
        state.send_replace(AudioState::Disconnected(reason));
        sleep(backoff).await;
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

/// Returns why the connection ended and whether any audio arrived.
async fn pump_audio(
    neg: Negotiated,
    packets: &mpsc::Sender<Bytes>,
    state: &watch::Sender<AudioState>,
) -> (String, bool) {
    let Negotiated {
        peer: Peer {
            mut rtc,
            socket,
            local,
        },
        mut remote,
        _signaling,
    } = neg;
    tracing::info!(%local, "webrtc audio peer up");
    let mut buf = vec![0u8; 2048];
    let mut got_any = false;
    loop {
        let deadline = loop {
            match rtc.poll_output() {
                Err(e) => return (e.to_string(), got_any),
                Ok(Output::Timeout(t)) => break t,
                Ok(Output::Transmit(t)) => {
                    let _ = socket.send_to(&t.contents, t.destination).await;
                }
                Ok(Output::Event(Event::IceConnectionStateChange(
                    IceConnectionState::Disconnected,
                ))) => return ("ICE disconnected".into(), got_any),
                Ok(Output::Event(Event::IceConnectionStateChange(st))) => {
                    tracing::info!(?st, "audio ice state");
                }
                Ok(Output::Event(Event::Connected)) => tracing::info!("audio dtls connected"),
                Ok(Output::Event(Event::PeerStats(p))) => {
                    tracing::debug!(
                        rx = p.peer_bytes_rx,
                        media_rx = p.bytes_rx,
                        "audio peer stats"
                    );
                }
                Ok(Output::Event(Event::MediaIngressStats(m))) => {
                    tracing::debug!(packets = m.packets, "audio media stats");
                }
                Ok(Output::Event(Event::MediaAdded(m))) => {
                    tracing::info!(kind = ?m.kind, "audio media added");
                }
                Ok(Output::Event(Event::MediaData(d))) => {
                    if !got_any {
                        got_any = true;
                        state.send_replace(AudioState::Streaming);
                    }
                    match packets.try_send(Bytes::copy_from_slice(&d.data)) {
                        Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => {}
                        Err(mpsc::error::TrySendError::Closed(_)) => {
                            return ("consumer closed".into(), got_any);
                        }
                    }
                }
                Ok(Output::Event(_)) => {}
            }
        };
        let input = tokio::select! {
            biased;
            cand = remote.recv() => {
                if let Some(c) = cand { rtc.add_remote_candidate(c); }
                continue;
            },
            r = socket.recv_from(&mut buf) => match r {
                Ok((n, source)) => match (&buf[..n]).try_into() {
                    Ok(contents) => Input::Receive(Instant::now(), Receive {
                        proto: Protocol::Udp, source, destination: local, contents,
                    }),
                    Err(_) => continue,
                },
                Err(e) => return (e.to_string(), got_any),
            },
            _ = sleep_until(tokio::time::Instant::from_std(deadline)) => Input::Timeout(Instant::now()),
            _ = packets.closed() => return ("consumer closed".into(), got_any),
        };
        if let Err(e) = rtc.handle_input(input) {
            return (e.to_string(), got_any);
        }
    }
}
