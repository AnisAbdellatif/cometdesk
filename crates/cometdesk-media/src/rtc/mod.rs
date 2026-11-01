//! Shared WebRTC engine for the Pion and Janus sources: a str0m peer driven over one UDP
//! socket, H.264 frames out, PLI in. Signaling lives in `pion` and `janus`.
//!
//! str0m has no STUN/TURN client, so only host candidates are offered: the device must be
//! reachable over UDP (LAN, VPN). Loss on the Pion video is repaired by FlexFEC-03 (see
//! `flexfec`; str0m only hands us the packets through the patch in `vendor/str0m`).

use std::future::Future;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket as StdUdp};
use std::time::{Duration, Instant};

use bytes::Bytes;
use cometdesk::{Error, Result};
use str0m::media::{KeyframeRequestKind, MediaKind, Mid};
use str0m::net::{Protocol, Receive};
use str0m::rtp::{ExtensionValues, RtpHeader};
use str0m::{Candidate, Event, IceConnectionState, Input, Output, Rtc, RtcConfig};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{MissedTickBehavior, interval, sleep, sleep_until};

use crate::annexb;
use crate::flexfec;
use crate::source::{Control, Ends, FrameTracker, MediaStats, SourceState};
use crate::{Codec, EncodedFrame};

/// Keyframe requests after lost packets are spaced at least this far apart.
const PLI_INTERVAL: Duration = Duration::from_millis(500);
/// A repair packet follows its group of 16 media packets by up to 4 packets, so a gap can
/// still be filled by FEC for that long; hold the next frames back just that much.
pub(crate) const REORDERING_FEC: usize = 20;
/// How long a frame may wait for missing packets. The engine's default is 2 s, which showed
/// up as 2 s freezes after every loss FEC could not repair; FEC or a retransmission has
/// arrived long before this.
const REORDERING_TIMEOUT: Duration = Duration::from_millis(250);
/// Without FEC a gap is only ever filled by a retransmission or not at all.
pub(crate) const REORDERING_PLAIN: usize = 30;
/// No frame for this long: ask for a keyframe (the engine drops frames behind a broken
/// keyframe until the next one).
const IDLE_KEY_REQUEST: Duration = Duration::from_millis(300);
const STALL_KEY_REQUEST: Duration = Duration::from_secs(3);
const STALL_RECONNECT: Duration = Duration::from_secs(5);
const BACKOFF_MIN: Duration = Duration::from_millis(250);
const BACKOFF_MAX: Duration = Duration::from_secs(10);

/// Test hook: `COMETDESK_TEST_RX_LOSS=<percent>` drops that share of incoming video datagrams
/// (before the engine sees them), to exercise loss recovery on a clean link.
struct RxLoss {
    threshold: u64,
    state: u64,
}

impl RxLoss {
    fn from_env() -> Option<RxLoss> {
        let pct: f64 = std::env::var("COMETDESK_TEST_RX_LOSS").ok()?.parse().ok()?;
        (pct > 0.0).then(|| RxLoss {
            threshold: (pct / 100.0 * u64::MAX as f64) as u64,
            state: 0x9e37_79b9_7f4a_7c15,
        })
    }

    fn drops(&mut self) -> bool {
        // xorshift64*
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        self.state.wrapping_mul(0x2545_f491_4f6c_dd1d) < self.threshold
    }
}

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
    ///
    /// `reordering`: packets the receiver holds back behind a gap before giving up on it.
    /// Each one costs 1/60 s of added delay at 60 fps when something is lost.
    pub async fn new(host: &str, reordering: usize, prefer: Codec) -> Result<Peer> {
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
        let mut rtc = configure_rtc(reordering, prefer);
        let cand = Candidate::host(local, "udp").map_err(proto)?;
        rtc.add_local_candidate(cand);
        Ok(Peer { rtc, socket, local })
    }
}

pub(crate) fn configure_rtc(reordering: usize, prefer: Codec) -> Rtc {
    let mut cfg = RtcConfig::new().clear_codecs();
    if prefer == Codec::H265 {
        cfg = cfg.enable_h265(true);
    }
    cfg.enable_h264(true)
        .enable_opus(true, false)
        .set_reordering_size_video(reordering)
        .set_reordering_timeout_video(Some(REORDERING_TIMEOUT))
        .set_stats_interval(Some(Duration::from_secs(1)))
        .build(Instant::now())
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

pub(crate) fn is_keyframe(codec: Codec, data: &[u8]) -> bool {
    annexb::nal_units(data).any(|n| {
        matches!(
            annexb::classify(codec, n),
            annexb::NalKind::Slice { idr: true }
        )
    })
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
    let mut tracker = FrameTracker::new();
    let mut watchdog = interval(Duration::from_millis(100));
    watchdog.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut got_any = false;
    let mut last_key: Option<Instant> = None;
    let mut last_pli: Option<Instant> = None;
    let mut fec = flexfec::Decoder::new();
    let mut loss = RxLoss::from_env();
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
                    Event::PeerStats(p) => tracing::debug!(
                        rx = p.peer_bytes_rx, loss = ?p.ingress_loss_fraction, rtt = ?p.rtt,
                        pair = ?p.selected_candidate_pair, "peer stats"),
                    Event::MediaIngressStats(m) => tracing::debug!(
                        packets = m.packets, plis = m.plis, nacks = m.nacks, loss = ?m.loss,
                        jitter = m.jitter, "media stats"),
                    Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                        end!("ICE disconnected")
                    }
                    Event::FecRtp(f) => {
                        let h = &f.header;
                        let out = if f.repair {
                            fec.repair(&f.payload)
                        } else {
                            fec.media(
                                (h.has_padding as u8) << 5
                                    | (h.has_extension as u8) << 4
                                    | h.csrc_count as u8,
                                (h.marker as u8) << 7 | *h.payload_type,
                                h.sequence_number,
                                h.timestamp,
                                &f.tail,
                                &f.payload,
                            )
                        };
                        for r in out {
                            let header = RtpHeader {
                                version: 2,
                                has_padding: false,
                                has_extension: false,
                                csrc_count: 0,
                                marker: r.marker,
                                payload_type: r.payload_type.into(),
                                sequence_number: r.sequence_number,
                                timestamp: r.timestamp,
                                ssrc: r.ssrc.into(),
                                csrc: [0; 15],
                                ext_vals: ExtensionValues::default(),
                                header_len: 12,
                            };
                            tracing::trace!(seq = r.sequence_number, "packet rebuilt from FEC");
                            rtc.inject_rtp(Instant::now(), header, &r.payload);
                        }
                    }
                    Event::MediaAdded(m) if m.kind == MediaKind::Video => {
                        mid = Some(m.mid);
                        pli!();
                    }
                    Event::MediaData(d) => {
                        mid = Some(d.mid);
                        let now = Instant::now();
                        let codec = match d.params.spec().codec {
                            str0m::format::Codec::H265 => Codec::H265,
                            _ => Codec::H264,
                        };
                        let key = is_keyframe(codec, &d.data);
                        if key {
                            tracing::debug!(
                                since_last = ?last_key.map(|t| now.duration_since(t)),
                                len = d.data.len(), "keyframe");
                            last_key = Some(now);
                        }
                        if !d.contiguous && !key {
                            // Packets are missing that FEC could not rebuild. The device
                            // answers keyframe requests slowly or not at all and a static
                            // screen can go many seconds without one, so keep decoding (the
                            // picture may show artifacts where the lost frame changed
                            // something) instead of freezing until the next keyframe.
                            stats.gaps_total += 1;
                            if last_pli.is_none_or(|t| t.elapsed() >= PLI_INTERVAL) {
                                last_pli = Some(Instant::now());
                                pli!();
                            }
                            tracing::debug!(len = d.data.len(), "gap in the video stream");
                        }
                        if waiting_for_key && !key {
                            stats.dropped_total += 1;
                            continue;
                        }
                        waiting_for_key = false;
                        if !got_any || key_requested {
                            got_any = true;
                            stats.codec = Some(codec);
                            ends.stats.send_replace(*stats);
                            ends.state.send_replace(SourceState::Streaming {
                                codec,
                                profile: None,
                            });
                        }
                        key_requested = false;
                        if let Some(prev) = last_frame
                            && now.duration_since(prev) > Duration::from_millis(300)
                        {
                            tracing::debug!(gap = ?now.duration_since(prev), key, contiguous = d.contiguous, len = d.data.len(), "late frame");
                        }
                        last_frame = Some(now);
                        tracker.record_frame(stats, key, d.data.len());
                        let frame = EncodedFrame {
                            codec,
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
                        if tracker.tick(stats, now) {
                            stats.codec = Some(codec);
                            ends.stats.send_replace(*stats);
                            let f = fec.stats();
                            tracing::debug!(
                                fps = stats.fps,
                                kbps = stats.kbps,
                                fec_repair = f.repair,
                                fec_recovered = f.recovered,
                                fec_inconsistent = f.inconsistent,
                                "rtc video stats"
                            );
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
                    // Keep the handshake (STUN, DTLS) intact; only RTP-looking datagrams
                    // (first byte 0x80..0xbf) are dropped.
                    Ok(_) if buf[0] >> 6 == 2 && loss.as_mut().is_some_and(RxLoss::drops) => continue,
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
                if got_any
                    && idle >= IDLE_KEY_REQUEST
                    && last_pli.is_none_or(|t| t.elapsed() >= PLI_INTERVAL)
                {
                    last_pli = Some(Instant::now());
                    pli!();
                    tracing::debug!(?idle, "no frames, asking for a keyframe");
                }
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

mod audio;
pub(crate) use audio::supervise_audio;

#[cfg(test)]
mod tests;
