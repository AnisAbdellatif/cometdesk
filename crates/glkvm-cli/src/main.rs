//! `glkvm`: command-line client for GLKVM devices.
//!
//! Credentials come from the environment or a `.env` file (values taken literally):
//! `GLKVM_HOST`, `GLKVM_USER` (default `admin`) and `GLKVM_PASSWORD`. With 2FA enabled, pass
//! `--totp CODE` (or `GLKVM_TOTP`), or type the code when asked.

mod commands;
mod renderer;
mod viewer;

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use glkvm::api::atx::{AtxButton, PowerAction};
use glkvm::api::streamer::{QualityPreset, StreamerUpdate};
use glkvm::api::system::{LatencyMode, Orientation, VencMode, VideoMode};
use glkvm::events::{EventOptions, EventSession};
use glkvm::tls::PinStore;
use glkvm::{Credentials, Session};
use glkvm_media::direct::{self, DirectOptions};
use glkvm_media::{Codec, SourceOptions, Transport};

use glkvm_view::keymap::{self, KeyOptions};
use glkvm_view::{InputMapper, RenderPath, ScrollOptions};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Device address (host or host:port). Defaults to $GLKVM_HOST.
    #[arg(long, global = true)]
    host: Option<String>,
    /// User name. Defaults to $GLKVM_USER, then "admin".
    #[arg(long, global = true)]
    user: Option<String>,
    /// Current 2FA (TOTP) code. Defaults to $GLKVM_TOTP; asked for when needed.
    #[arg(long, global = true)]
    totp: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Open a console window: video + keyboard and mouse.
    View {
        /// Send Cmd as Ctrl and Ctrl as Cmd (for Windows/Linux targets).
        #[arg(long)]
        swap_cmd_ctrl: bool,
        /// Fix the swapped ISO keys (§/^ and </>) that macOS reports on ISO keyboards.
        #[arg(long, value_enum, default_value_t = Toggle::Auto)]
        iso_fix: Toggle,
        /// Wheel units per scroll step. Defaults to the device's `scroll_rate`.
        #[arg(long)]
        scroll_rate: Option<u32>,
        /// Preferred codec when the device offers both.
        #[arg(long, value_enum, default_value_t = CodecArg::H264)]
        codec: CodecArg,
        /// How video reaches us: direct (`/api/media/ws`), pion (WebRTC FEC), janus (WebRTC).
        #[arg(long, value_enum, default_value_t = TransportArg::Direct)]
        transport: TransportArg,
        /// Play the target's audio (device in webrtc or flex_fec mode; opens a second WebRTC connection).
        #[arg(long)]
        audio: bool,
        /// How decoded frames reach the GPU.
        #[arg(long, value_enum, default_value_t = RenderArg::Auto)]
        render: RenderArg,
        /// Measure CPU for this many seconds (after a 3 s warm-up), print the result and exit.
        #[arg(long, value_name = "SECONDS")]
        bench: Option<u64>,
    },
    /// Print protocol details: streamer state, media formats and the first frames.
    Probe {
        #[arg(long, default_value_t = 10)]
        frames: usize,
    },
    /// Model, firmware, features and target status.
    Info,
    /// Save a JPEG snapshot of the target's screen.
    Snapshot {
        #[arg(default_value = "snapshot.jpg")]
        out: PathBuf,
        /// Scale down to fit WIDTHxHEIGHT.
        #[arg(long, value_parser = parse_size)]
        preview: Option<(u32, u32)>,
    },
    /// ATX power action on the target (asks for confirmation).
    Power {
        #[arg(value_enum)]
        action: PowerArg,
        /// Don't ask for confirmation.
        #[arg(long)]
        yes: bool,
    },
    /// Press an ATX button (asks for confirmation).
    Click {
        #[arg(value_enum)]
        button: ButtonArg,
        #[arg(long)]
        yes: bool,
    },
    /// Show the device's saved video mode, or change it (device-wide, asks for confirmation).
    VideoMode {
        #[arg(value_enum)]
        mode: Option<VideoModeArg>,
        #[arg(long)]
        yes: bool,
    },
    /// Show the encoder mode, or change it (device-wide, asks for confirmation).
    VencMode {
        #[arg(value_enum)]
        mode: Option<VencModeArg>,
        #[arg(long)]
        yes: bool,
    },
    /// Show the latency mode, or change it (device-wide, asks for confirmation).
    LatencyMode {
        #[arg(value_enum)]
        mode: Option<LatencyModeArg>,
        #[arg(long)]
        yes: bool,
    },
    /// Show the picture rotation, or change it (device-wide, asks for confirmation).
    Orientation {
        /// Degrees.
        #[arg(value_parser = clap::builder::PossibleValuesParser::new(["0", "90", "180", "270"]))]
        degrees: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// List the display (EDID) presets, or switch to one by key (device-wide, asks for
    /// confirmation; the target sees a monitor change).
    Edid {
        /// Preset key from the list, e.g. E1920x1080.
        key: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// Show or change encoder quality (device-wide, asks for confirmation).
    Quality {
        /// Named level; sets bitrate (inside the device limits) and fps.
        #[arg(value_enum)]
        preset: Option<PresetArg>,
        /// Frames per second.
        #[arg(long)]
        fps: Option<i64>,
        /// H.264 bitrate in kbit/s.
        #[arg(long)]
        bitrate: Option<i64>,
        /// Keyframe interval in frames.
        #[arg(long)]
        gop: Option<i64>,
        /// JPEG quality (1-100).
        #[arg(long)]
        jpeg_quality: Option<i64>,
        #[arg(long)]
        yes: bool,
    },
    /// Send a Wake-on-LAN packet to a MAC address or a saved target name.
    Wake { target: String },
    /// List saved Wake-on-LAN targets.
    WolList,
    /// Type text on the target (from the argument or stdin).
    Type {
        text: Option<String>,
        /// Target keyboard layout (see the device's keymaps); device default if omitted.
        #[arg(long)]
        keymap: Option<String>,
        /// Type slowly (BIOS/UEFI).
        #[arg(long)]
        slow: bool,
    },
    /// Record scrubbed read-only API responses as test fixtures.
    RecordFixtures { dir: PathBuf },
    /// Virtual media: images stored on the device, mounted as a CD-ROM or USB drive.
    Msd {
        #[command(subcommand)]
        cmd: MsdCmd,
    },
    /// Clipboard bridge on the device (docs/clipboard-bridge.md).
    Bridge {
        #[command(subcommand)]
        cmd: BridgeCmd,
    },
}

#[derive(Subcommand)]
enum MsdCmd {
    /// Show the stored images and what the target sees.
    Status,
    /// Upload an image file to the device's storage.
    Upload {
        file: PathBuf,
        /// Name on the device. Defaults to the file name.
        #[arg(long)]
        name: Option<String>,
    },
    /// Connect an image to the target (device-wide, asks for confirmation).
    Mount {
        /// Image name as listed by `msd status`.
        image: String,
        /// Present it as a USB flash drive instead of a CD-ROM.
        #[arg(long)]
        flash: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Disconnect the drive from the target.
    Unmount {
        #[arg(long)]
        yes: bool,
    },
    /// Delete a stored image.
    Remove {
        image: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum BridgeCmd {
    /// Show whether the bridge is installed and healthy.
    Status,
    /// Install, update or repair the bridge, then upload the agent binaries.
    /// May restart the device's USB functions (the target's keyboard drops for a moment).
    Setup {
        /// Directory with agent binaries (scripts/build-agents.sh).
        #[arg(long, default_value = "dist/agents")]
        agents: PathBuf,
    },
    /// Remove the bridge and its USB network function from the device.
    Remove {
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum PowerArg {
    On,
    Off,
    OffHard,
    ResetHard,
}

#[derive(Clone, Copy, ValueEnum)]
enum ButtonArg {
    Power,
    PowerLong,
    Reset,
}

fn parse_size(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s.split_once(['x', 'X']).ok_or("expected WIDTHxHEIGHT")?;
    Ok((
        w.parse().map_err(|_| "bad width")?,
        h.parse().map_err(|_| "bad height")?,
    ))
}

#[derive(Clone, Copy, ValueEnum)]
enum Toggle {
    Auto,
    On,
    Off,
}

#[derive(Clone, Copy, ValueEnum)]
enum RenderArg {
    /// Zero-copy if available, otherwise upload.
    Auto,
    /// Copy each frame into GPU textures.
    Upload,
    /// Sample the decoder's IOSurfaces directly (macOS).
    ZeroCopy,
}

#[derive(Clone, Copy, ValueEnum)]
enum PresetArg {
    Low,
    Medium,
    High,
    Max,
}

#[derive(Clone, Copy, ValueEnum)]
enum VideoModeArg {
    /// WebRTC with FlexFEC (Pion).
    FlexFec,
    /// Classic WebRTC (Janus).
    Webrtc,
    /// GL native adaptive mode (disables Janus).
    GlWebrtc,
    DirectH264,
}

#[derive(Clone, Copy, ValueEnum)]
enum VencModeArg {
    /// Reduces bandwidth.
    Smart,
    Normal,
}

#[derive(Clone, Copy, ValueEnum)]
enum LatencyModeArg {
    LowestLatency,
    SmoothDisplay,
}

#[derive(Clone, Copy, ValueEnum)]
enum TransportArg {
    Direct,
    /// WebRTC (FEC): GL's Pion server.
    Pion,
    /// WebRTC: Janus + uStreamer (needs `gl_webrtc` off).
    Janus,
}

#[derive(Clone, Copy, ValueEnum)]
enum CodecArg {
    H264,
    H265,
}

fn main() -> Result<()> {
    glkvm::env::load_dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let cli = Cli::parse();

    let host = cli
        .host
        .or_else(|| std::env::var("GLKVM_HOST").ok())
        .context("no device: pass --host or set GLKVM_HOST")?;
    let user = cli
        .user
        .or_else(|| std::env::var("GLKVM_USER").ok())
        .unwrap_or_else(|| "admin".into());
    let password = std::env::var("GLKVM_PASSWORD").context("set GLKVM_PASSWORD (e.g. in .env)")?;
    let mut creds = Credentials::new(user, password);
    creds.totp = cli.totp.or_else(|| std::env::var("GLKVM_TOTP").ok());

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("glkvm-io")
        .enable_all()
        .build()?;
    let pins = Arc::new(PinStore::load_default()?);
    let session = rt.block_on(login(&host, creds, pins)).context("login")?;
    tracing::info!(host = %session.host(), "logged in");

    match cli.cmd {
        Cmd::View {
            swap_cmd_ctrl,
            iso_fix,
            scroll_rate,
            codec,
            transport,
            audio,
            render,
            bench,
        } => {
            let render = match render {
                RenderArg::Auto => RenderPath::Auto,
                RenderArg::Upload => RenderPath::Upload,
                RenderArg::ZeroCopy => RenderPath::ZeroCopy,
            };
            let input = (swap_cmd_ctrl, iso_fix, scroll_rate);
            view(
                &rt,
                session,
                input,
                (codec, transport, audio),
                render,
                bench.map(Duration::from_secs),
            )
        }
        Cmd::Probe { frames } => rt.block_on(probe(session, frames)),
        cmd => rt.block_on(async {
            let result = one_shot(&session, cmd).await;
            let _ = session.logout().await;
            result
        }),
    }
}

/// Logs in, asking for a TOTP code on the terminal if the device needs one.
async fn login(host: &str, mut creds: Credentials, pins: Arc<PinStore>) -> Result<Session> {
    match Session::login(host, &creds, pins.clone()).await {
        Err(glkvm::Error::TotpRequired) if std::io::stdin().is_terminal() => {
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

async fn one_shot(session: &Session, cmd: Cmd) -> Result<()> {
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

fn view(
    rt: &tokio::runtime::Runtime,
    session: Session,
    (swap_cmd_ctrl, iso_fix, scroll_rate): (bool, Toggle, Option<u32>),
    (codec, transport, audio): (CodecArg, TransportArg, bool),
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
            "device video mode may not serve this transport; `glkvm-cli video-mode <mode>` changes it (device-wide)"
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
        CodecArg::H264 => Codec::H264,
        CodecArg::H265 => Codec::H265,
    };
    let transport = match transport {
        TransportArg::Direct => Transport::Direct,
        TransportArg::Pion => Transport::Pion,
        TransportArg::Janus => Transport::Janus,
    };
    let media = glkvm_media::start(session.clone(), SourceOptions { transport, prefer });
    // Kept alive until the viewer closes.
    let _audio = audio
        .then(|| {
            let Some(source) = glkvm_media::audio::start(session.clone(), mode) else {
                tracing::warn!(
                    mode,
                    "no audio in this video mode (needs webrtc or flex_fec)"
                );
                return None;
            };
            let (packets, source) = source.take_packets();
            match glkvm_media::audio::Player::start(packets, None) {
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
        title: format!("glkvm {}", session.host()),
        render,
        bench,
    };
    let result = viewer::run(parts);
    rt.block_on(async {
        let _ = tokio::time::timeout(Duration::from_secs(1), events.close()).await;
    });
    result
}

async fn probe(session: Session, frames: usize) -> Result<()> {
    use glkvm_media::annexb::{nal_type, nal_units};

    let config = session.system_config().await?;
    println!("system config: {config:?}");
    let streamer = session.streamer_state().await?;
    println!("streamer (before /api/ws): {:?}", streamer.streamer);

    let events = EventSession::spawn(session.clone(), EventOptions::default());
    let mut sub = events.subscribe();
    let mut conn = events.conn_state();
    tokio::time::timeout(
        Duration::from_secs(5),
        conn.wait_for(|s| *s == glkvm::events::ConnState::Connected),
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
    }
    println!("media state: {:?}", *media.state.borrow());
    let streamer = session.streamer_state().await?;
    println!("streamer (while viewing): {:?}", streamer.streamer);
    drop(media);
    events.close().await;
    Ok(())
}
