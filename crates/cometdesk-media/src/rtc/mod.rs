//! Shared WebRTC engine for the Pion and Janus sources: a str0m peer driven over one UDP
//! socket, H.264 frames out, PLI in. Signaling lives in `pion` and `janus`;
//! the video pump is in `video.rs`, the audio pump in `audio.rs`.
//!
//! str0m has no STUN/TURN client, so only host candidates are offered: the device must be
//! reachable over UDP (LAN, VPN). Loss on the Pion video is repaired by FlexFEC-03 (see
//! `flexfec`; str0m only hands us the packets through the patch in `vendor/str0m`).

use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket as StdUdp};
use std::time::{Duration, Instant};

use cometdesk::{Error, Result};
use str0m::{Candidate, Rtc, RtcConfig};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::Codec;
use crate::annexb;

/// A repair packet follows its group of 16 media packets by up to 4 packets, so a gap can
/// still be filled by FEC for that long; hold the next frames back just that much.
pub(crate) const REORDERING_FEC: usize = 20;
/// How long a frame may wait for missing packets. The engine's default is 2 s, which showed
/// up as 2 s freezes after every loss FEC could not repair; FEC or a retransmission has
/// arrived long before this.
const REORDERING_TIMEOUT: Duration = Duration::from_millis(250);
/// Without FEC a gap is only ever filled by a retransmission or not at all.
pub(crate) const REORDERING_PLAIN: usize = 30;
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
mod audio;
mod video;
pub(crate) use audio::supervise_audio;
pub(crate) use video::supervise;

#[cfg(test)]
mod tests;
