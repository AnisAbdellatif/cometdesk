//! Session login and command dispatch (`one_shot`, `view`, `probe`).
//!
//! Entry points: `login`, `one_shot`, `view`, `probe`.
//! Maps parsed `args` types to `commands` or interactive console/probe sessions.

use std::io::{IsTerminal, Write};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use cometdesk::api::atx::{AtxButton, PowerAction};
use cometdesk::api::streamer::{QualityPreset, StreamerUpdate};
use cometdesk::api::system::{LatencyMode, Orientation, VencMode, VideoFormat, VideoMode};
use cometdesk::events::{EventOptions, EventSession};
use cometdesk::tls::PinStore;
use cometdesk::{Credentials, Session};
use cometdesk_media::direct::{self, DirectOptions};
use cometdesk_media::{Codec, SourceOptions, Transport};
use cometdesk_view::keymap::{self, KeyOptions};
use cometdesk_view::{InputMapper, RenderPath, ScrollOptions};

use crate::args::{
    BridgeCmd, ButtonArg, Cmd, CodecArg, LatencyModeArg, MsdCmd, PowerArg, PresetArg, Toggle,
    TransportArg, VencModeArg, VideoModeArg,
};
use crate::{commands, viewer};

/// Logs in, asking for a TOTP code on the terminal if the device needs one.
pub(crate) async fn login(
    host: &str,
    mut creds: Credentials,
    pins: Arc<PinStore>,
) -> Result<Session> {
    match Session::login(host, &creds, pins.clone()).await {
        Err(cometdesk::Error::TotpRequired) if std::io::stdin().is_terminal() => {
            print!("2FA code: ");
            std::io::stdout().flush()?;
            let mut code = String::new();
            std::io::stdin().read_line(&mut code)?;
            creds.totp = Some(code.trim().to_owned());
            Ok(Session::login(host, &creds, pins).await?)
        }
        other => Ok(other?),
    }
}

pub(crate) async fn one_shot(session: &Session, cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::Info => commands::info(session).await,
        Cmd::Snapshot { out, preview } => commands::snapshot(session, &out, preview).await,
        Cmd::Power { action, yes } => {
            let action = match action {
                PowerArg::On => PowerAction::On,
                PowerArg::Off => PowerAction::Off,
                PowerArg::OffHard => PowerAction::OffHard,
                PowerArg::ResetHard => PowerAction::ResetHard,
            };
            commands::power(session, action, yes).await
        }
        Cmd::Click { button, yes } => {
            let button = match button {
                ButtonArg::Power => AtxButton::Power,
                ButtonArg::PowerLong => AtxButton::PowerLong,
                ButtonArg::Reset => AtxButton::Reset,
            };
            commands::click(session, button, yes).await
        }
        Cmd::Quality {
            preset,
            fps,
            bitrate,
            gop,
            jpeg_quality,
            yes,
        } => {
            let preset = preset.map(|p| match p {
                PresetArg::Low => QualityPreset::Low,
                PresetArg::Medium => QualityPreset::Medium,
                PresetArg::High => QualityPreset::High,
                PresetArg::Max => QualityPreset::Max,
            });
            let manual = StreamerUpdate {
                desired_fps: fps,
                h264_bitrate: bitrate,
                h264_gop: gop,
                quality: jpeg_quality,
            };
            commands::quality(session, preset, manual, yes).await
        }
        Cmd::Codec { format, yes } => {
            let format = format.map(|f| match f {
                CodecArg::H264 => VideoFormat::H264,
                CodecArg::H265 => VideoFormat::H265,
            });
            commands::codec(session, format, yes).await
        }
        Cmd::VencMode { mode, yes } => {
            let mode = mode.map(|m| match m {
                VencModeArg::Smart => VencMode::Smart,
                VencModeArg::Normal => VencMode::Normal,
            });
            commands::venc_mode(session, mode, yes).await
        }
        Cmd::LatencyMode { mode, yes } => {
            let mode = mode.map(|m| match m {
                LatencyModeArg::LowestLatency => LatencyMode::LowestLatency,
                LatencyModeArg::SmoothDisplay => LatencyMode::SmoothDisplay,
            });
            commands::latency_mode(session, mode, yes).await
        }
        Cmd::Orientation { degrees, yes } => {
            let o = degrees
                .map(|d| d.parse().ok().and_then(Orientation::from_degrees))
                .map(|o| o.context("orientation must be 0, 90, 180 or 270"))
                .transpose()?;
            commands::orientation(session, o, yes).await
        }
        Cmd::Edid { key, yes } => commands::edid(session, key.as_deref(), yes).await,
        Cmd::VideoMode { mode, yes } => {
            let mode = mode.map(|m| match m {
                VideoModeArg::FlexFec => VideoMode::FlexFec,
                VideoModeArg::Webrtc => VideoMode::Webrtc,
                VideoModeArg::GlWebrtc => VideoMode::GlWebrtc,
                VideoModeArg::DirectH264 => VideoMode::DirectH264,
            });
            commands::video_mode(session, mode, yes).await
        }
        Cmd::Wake { target } => commands::wake(session, &target).await,
        Cmd::WolList => commands::wol_list(session).await,
        Cmd::Type { text, keymap, slow } => {
            commands::type_text(session, text, keymap.as_deref(), slow).await
        }
        Cmd::RecordFixtures { dir } => commands::record_fixtures(session, &dir).await,
        Cmd::Msd { cmd } => match cmd {
            MsdCmd::Status => commands::msd_status(session).await,
            MsdCmd::Upload { file, name } => commands::msd_upload(session, &file, name).await,
            MsdCmd::Mount { image, flash, yes } => {
                commands::msd_mount(session, &image, !flash, yes).await
            }
            MsdCmd::Unmount { yes } => commands::msd_unmount(session, yes).await,
            MsdCmd::Remove { image, yes } => commands::msd_remove(session, &image, yes).await,
        },
        Cmd::Bridge { cmd } => match cmd {
            BridgeCmd::Status => commands::bridge_status(session).await,
            BridgeCmd::Setup { agents } => commands::bridge_setup(session, &agents).await,
            BridgeCmd::Remove { yes } => commands::bridge_remove(session, yes).await,
        },
        Cmd::View { .. } | Cmd::Probe { .. } => unreachable!("handled in main"),
    }
}

pub(crate) fn view(
    rt: &tokio::runtime::Runtime,
    session: Session,
    (swap_cmd_ctrl, iso_fix, scroll_rate): (bool, Toggle, Option<u32>),
    (codec, transport, audio): (Option<CodecArg>, TransportArg, bool),
    render: RenderPath,
    bench: Option<Duration>,
) -> Result<()> {
    let config = rt.block_on(session.system_config()).unwrap_or_else(|e| {
        tracing::warn!(%e, "cannot read device config, using defaults");
        Default::default()
    });
    let mode = config.video_mode.as_deref().unwrap_or("unknown");
    let fits = match transport {
        TransportArg::Direct => true,
        TransportArg::Pion => matches!(mode, "flex_fec" | "webrtc" | "unknown"),
        TransportArg::Janus => matches!(mode, "webrtc" | "unknown"),
    };
    if fits {
        tracing::info!(video_mode = mode, "device video mode");
    } else {
        tracing::warn!(
            video_mode = mode,
            "device video mode may not serve this transport; `cometdesk-cli video-mode <mode>` changes it (device-wide)"
        );
    }
    let iso = match iso_fix {
        Toggle::On => true,
        Toggle::Off => false,
        Toggle::Auto => keymap::host_keyboard_is_iso(),
    };
    let keys = KeyOptions {
        iso_fix: iso,
        swap_cmd_ctrl,
    };
    let mut scroll =
        ScrollOptions::from_config(config.scroll_rate, config.reverse_scrolling.as_deref());
    if let Some(rate) = scroll_rate {
        scroll.rate = rate.clamp(1, 127) as i32;
    }
    tracing::info!(?keys, ?scroll, "input options");

    let _guard = rt.enter();
    // Streaming on: the device only runs its video streamer while such a session exists.
    let events = EventSession::spawn(session.clone(), EventOptions::default());
    let prefer = match codec {
        Some(CodecArg::H264) => Codec::H264,
        Some(CodecArg::H265) => Codec::H265,
        None => match config.video_format {
            Some(1) => Codec::H265,
            _ => Codec::H264,
        },
    };
    let transport = match transport {
        TransportArg::Direct => Transport::Direct,
        TransportArg::Pion => Transport::Pion,
        TransportArg::Janus => Transport::Janus,
    };
    let media = cometdesk_media::start(session.clone(), SourceOptions { transport, prefer });
    // Kept alive until the viewer closes.
    let _audio = audio
        .then(|| {
            let Some(source) = cometdesk_media::audio::start(session.clone(), mode) else {
                tracing::warn!(
                    mode,
                    "no audio in this video mode (needs webrtc, flex_fec or direct_h264)"
                );
                return None;
            };
            let (packets, source) = source.take_packets();
            match cometdesk_media::audio::Player::start(packets, None) {
                Ok(player) => Some((player, source)),
                Err(e) => {
                    tracing::warn!(%e, "audio disabled");
                    None
                }
            }
        })
        .flatten();

    let parts = viewer::ViewerParts {
        media,
        hid: events.hid(),
        conn: events.conn_state(),
        mapper: InputMapper::new(keys, scroll),
        title: format!("cometdesk {}", session.host()),
        render,
        bench,
    };
    let result = viewer::run(parts);
    rt.block_on(async {
        let _ = tokio::time::timeout(Duration::from_secs(1), events.close()).await;
    });
    result
}

pub(crate) async fn probe(
    session: Session,
    frames: usize,
    key_request_at: Option<usize>,
) -> Result<()> {
    use cometdesk_media::annexb::{nal_type, nal_units};

    let config = session.system_config().await?;
    println!("system config: {config:?}");
    let streamer = session.streamer_state().await?;
    println!("streamer (before /api/ws): {:?}", streamer.streamer);

    let events = EventSession::spawn(session.clone(), EventOptions::default());
    let mut sub = events.subscribe();
    let mut conn = events.conn_state();
    tokio::time::timeout(
        Duration::from_secs(5),
        conn.wait_for(|s| *s == cometdesk::events::ConnState::Connected),
    )
    .await
    .context("event socket did not connect")??;
    println!("event socket connected");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while let Ok(Ok(ev)) = tokio::time::timeout_at(deadline, sub.recv()).await {
        // `turn` carries relay credentials: never print it.
        if ev.kind == "turn" {
            continue;
        }
        let text = ev.data.to_string();
        let max = text.len().min(300);
        let cut = (0..=max)
            .rev()
            .find(|&i| text.is_char_boundary(i))
            .unwrap_or(0);
        println!("event {:<12} {}", ev.kind, &text[..cut]);
    }
    let streamer = session.streamer_state().await?;
    println!("streamer (after /api/ws, 3 s): {:?}", streamer.streamer);

    let mut media = direct::start(session.clone(), DirectOptions::default());
    let mut prev = None;
    for i in 0..frames {
        let Some(f) = tokio::time::timeout(Duration::from_secs(10), media.video.recv()).await?
        else {
            bail!("media session ended");
        };
        let types: Vec<u8> = nal_units(&f.data).map(|n| nal_type(f.codec, n)).collect();
        let gap = prev.map(|p: std::time::Instant| f.received_at - p);
        prev = Some(f.received_at);
        println!(
            "frame {i:>3}: {:?} key={} {:>7} bytes  NAL types {:?}  annexb={}  gap={:?}",
            f.codec,
            f.keyframe,
            f.data.len(),
            types,
            f.data.starts_with(&[0, 0, 0, 1]) || f.data.starts_with(&[0, 0, 1]),
            gap,
        );
        if key_request_at == Some(i) {
            println!("requesting a keyframe");
            media.control.request_keyframe();
        }
    }
    println!("media state: {:?}", *media.state.borrow());
    let streamer = session.streamer_state().await?;
    println!("streamer (while viewing): {:?}", streamer.streamer);
    drop(media);
    events.close().await;
    Ok(())
}
