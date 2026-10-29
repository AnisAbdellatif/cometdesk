//! Shared WebRTC engine for the Pion and Janus sources: a str0m peer driven over one UDP
//! socket, H.264 frames out, PLI in. Signaling lives in `pion` and `janus`.
//!
//! str0m has no STUN/TURN client, so only host candidates are offered: the device must be
//! reachable over UDP (LAN, VPN). FlexFEC is not implemented; loss is repaired with NACK/RTX.

use std::future::Future;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket as StdUdp};
use std::time::{Duration, Instant};

use bytes::Bytes;
use glkvm::{Error, Result};
use str0m::media::{KeyframeRequestKind, MediaKind, Mid};
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc, RtcConfig};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{MissedTickBehavior, interval, sleep, sleep_until};

use crate::annexb;
use crate::audio::AudioState;
use crate::source::{Control, Ends, MediaStats, SourceState};
use crate::{Codec, EncodedFrame};

const STALL_KEY_REQUEST: Duration = Duration::from_secs(3);
const STALL_RECONNECT: Duration = Duration::from_secs(5);
const BACKOFF_MIN: Duration = Duration::from_millis(250);
const BACKOFF_MAX: Duration = Duration::from_secs(10);

pub(crate) fn proto(e: impl std::fmt::Display) -> Error {
    Error::Protocol(e.to_string())
}

/// An H.264 (or, on its own connection, Opus) receiver. Playout is "on arrival": str0m releases a frame as soon as
/// it is complete.
pub(crate) struct Peer {
    pub rtc: Rtc,
    socket: UdpSocket,
    local: SocketAddr,
}

impl Peer {
    /// Binds a UDP socket on the interface that routes to `host` and registers it as the
    /// only local candidate.
    pub async fn new(host: &str) -> Result<Peer> {
        let target = host_addr(host)?;
        let ip = tokio::task::spawn_blocking(move || -> std::io::Result<IpAddr> {
            let probe = StdUdp::bind(if target.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            })?;
            probe.connect(target)?;
            Ok(probe.local_addr()?.ip())
        })
        .await
        .map_err(proto)?
        .map_err(proto)?;
        let socket = UdpSocket::bind((ip, 0)).await.map_err(proto)?;
        let local = socket.local_addr().map_err(proto)?;
        let mut rtc = RtcConfig::new()
            .clear_codecs()
            .enable_h264(true)
            .enable_opus(true, false)
            .set_stats_interval(None)
            .build(Instant::now());
        let cand = Candidate::host(local, "udp").map_err(proto)?;
        rtc.add_local_candidate(cand);
        Ok(Peer { rtc, socket, local })
    }
}

fn host_addr(host: &str) -> Result<SocketAddr> {
    let with_port = if host
        .rsplit_once(':')
        .is_some_and(|(_, p)| p.parse::<u16>().is_ok())
    {
        host.to_owned()
    } else {
        format!("{host}:443")
    };
    with_port
        .to_socket_addrs()
        .map_err(proto)?
        .next()
        .ok_or_else(|| Error::Protocol(format!("cannot resolve {host}")))
}

/// Keeps a spawned signaling task alive for as long as the connection runs.
pub(crate) struct AbortOnDrop(pub JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// A finished handshake: media is about to flow.
pub(crate) struct Negotiated {
    pub peer: Peer,
    /// Remote ICE candidates still arriving by signaling.
    pub remote: mpsc::UnboundedReceiver<Candidate>,
    pub _signaling: AbortOnDrop,
}

/// Parses a candidate from either an SDP `candidate:` line or the bare attribute value.
pub(crate) fn parse_candidate(line: &str) -> Option<Candidate> {
    let line = line.trim().trim_start_matches("a=");
    let line = line.strip_prefix("candidate:").unwrap_or(line);
    Candidate::from_sdp_string(&format!("candidate:{line}")).ok()
}

pub(crate) fn is_keyframe(data: &[u8]) -> bool {
    annexb::nal_units(data).any(|n| annexb::nal_type(Codec::H264, n) == 5)
}

enum Ended {
    /// At least one frame arrived.
    Streamed(String),
    NoFrames(String),
}

/// Reconnect loop shared by both WebRTC sources.
pub(crate) async fn supervise<F, Fut>(name: &'static str, mut ends: Ends, connect: F)
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<Negotiated>>,
{
    let mut backoff = BACKOFF_MIN;
    let mut stats = MediaStats::default();
    loop {
        if ends.frames.is_closed() {
            ends.state.send_replace(SourceState::Closed);
            return;
        }
        ends.state.send_replace(SourceState::Connecting);
        let reason = match connect().await {
            Ok(neg) => match pump(neg, &mut ends, &mut stats).await {
                Ended::Streamed(r) => {
                    backoff = BACKOFF_MIN;
                    r
                }
                Ended::NoFrames(r) => r,
            },
            Err(e) => e.to_string(),
        };
        if ends.frames.is_closed() {
            ends.state.send_replace(SourceState::Closed);
            return;
        }
        tracing::warn!(%reason, retry_in = ?backoff, "{name} connection lost");
        ends.state.send_replace(SourceState::Disconnected(reason));
        sleep(backoff).await;
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

async fn pump(neg: Negotiated, ends: &mut Ends, stats: &mut MediaStats) -> Ended {
    let Negotiated {
        peer: Peer {
            mut rtc,
            socket,
            local,
        },
        mut remote,
        _signaling,
    } = neg;
    tracing::info!(%local, "webrtc peer up, waiting for media");
    let mut mid: Option<Mid> = None;
    let mut buf = vec![0u8; 2048];
    let started = Instant::now();
    let mut last_frame: Option<Instant> = None;
    let mut key_requested = false;
    let mut waiting_for_key = true;
    let mut window = (Instant::now(), 0u32, 0usize);
    let mut watchdog = interval(Duration::from_millis(500));
    watchdog.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut got_any = false;
    macro_rules! end {
        ($r:expr) => {
            return if got_any {
                Ended::Streamed($r.to_string())
            } else {
                Ended::NoFrames($r.to_string())
            }
        };
    }
    macro_rules! pli {
        () => {
            if let Some(mut w) = mid.and_then(|m| rtc.writer(m)) {
                let _ = w.request_keyframe(None, KeyframeRequestKind::Pli);
            }
        };
    }

    loop {
        // Drain the engine until it wants to wait.
        let deadline = loop {
            match rtc.poll_output() {
                Err(e) => end!(e),
                Ok(Output::Timeout(t)) => break t,
                Ok(Output::Transmit(t)) => {
                    let _ = socket.send_to(&t.contents, t.destination).await;
                }
                Ok(Output::Event(ev)) => match ev {
                    Event::IceConnectionStateChange(st)
                        if st != IceConnectionState::Disconnected =>
                    {
                        tracing::info!(?st, "ice state");
                    }
                    Event::Connected => tracing::info!("dtls connected"),
                    Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                        end!("ICE disconnected")
                    }
                    Event::MediaAdded(m) if m.kind == MediaKind::Video => {
                        mid = Some(m.mid);
                        pli!();
                    }
                    Event::MediaData(d) => {
                        mid = Some(d.mid);
                        let now = Instant::now();
                        let key = is_keyframe(&d.data);
                        if !d.contiguous && !key {
                            waiting_for_key = true;
                            pli!();
                        }
                        if waiting_for_key && !key {
                            stats.dropped_total += 1;
                            continue;
                        }
                        waiting_for_key = false;
                        if !got_any || key_requested {
                            got_any = true;
                            ends.state.send_replace(SourceState::Streaming {
                                codec: Codec::H264,
                                profile: None,
                            });
                        }
                        key_requested = false;
                        last_frame = Some(now);
                        stats.frames_total += 1;
                        window.1 += 1;
                        window.2 += d.data.len();
                        let frame = EncodedFrame {
                            codec: Codec::H264,
                            data: Bytes::copy_from_slice(&d.data),
                            keyframe: key,
                            received_at: now,
                        };
                        match ends.frames.try_send(frame) {
                            Ok(()) => {}
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                stats.dropped_total += 1;
                                waiting_for_key = true;
                                pli!();
                            }
                            Err(mpsc::error::TrySendError::Closed(_)) => end!("consumer closed"),
                        }
                        let el = now.duration_since(window.0);
                        if el >= Duration::from_secs(1) {
                            let secs = el.as_secs_f32();
                            stats.fps = window.1 as f32 / secs;
                            stats.kbps = window.2 as f32 * 8.0 / 1000.0 / secs;
                            ends.stats.send_replace(*stats);
                            window = (now, 0, 0);
                        }
                    }
                    _ => {}
                },
            }
        };

        let input = tokio::select! {
            biased;
            ctl = ends.ctl.recv() => match ctl {
                Some(Control::KeyFrame) => { pli!(); continue; }
                None => end!("controller dropped"),
            },
            cand = remote.recv() => {
                if let Some(c) = cand { tracing::info!(cand = %c, "remote candidate"); rtc.add_remote_candidate(c); }
                continue;
            },
            r = socket.recv_from(&mut buf) => match r {
                Ok((n, source)) => match (&buf[..n]).try_into() {
                    Ok(contents) => Input::Receive(Instant::now(), Receive {
                        proto: Protocol::Udp, source, destination: local, contents,
                    }),
                    Err(_) => continue,
                },
                Err(e) => end!(e),
            },
            _ = sleep_until(tokio::time::Instant::from_std(deadline)) => Input::Timeout(Instant::now()),
            _ = watchdog.tick() => {
                let idle = last_frame.unwrap_or(started).elapsed();
                if idle >= STALL_RECONNECT {
                    end!(if got_any { "stalled" } else { "no frames" });
                }
                if idle >= STALL_KEY_REQUEST && !key_requested {
                    key_requested = true;
                    waiting_for_key = true;
                    if got_any { ends.state.send_replace(SourceState::Stalled); }
                    pli!();
                }
                continue;
            }
        };
        if let Err(e) = rtc.handle_input(input) {
            end!(e);
        }
    }
}

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
