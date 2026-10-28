//! `glkvm`: command-line client for GLKVM devices.
//!
//! Credentials come from the environment or a `.env` file (values taken literally): `GLKVM_HOST`, `GLKVM_USER`
//! (default `admin`) and `GLKVM_PASSWORD`.

mod input;
mod keymap;
mod renderer;
mod viewer;
#[cfg(target_os = "macos")]
mod zerocopy;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use glkvm::events::{EventOptions, EventSession};
use glkvm::tls::PinStore;
use glkvm::{Credentials, Session};
use glkvm_media::Codec;
use glkvm_media::direct::{self, DirectOptions};

use crate::input::{InputMapper, ScrollOptions};
use crate::keymap::KeyOptions;
use crate::renderer::RenderPath;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Device address (host or host:port). Defaults to $GLKVM_HOST.
    #[arg(long, global = true)]
    host: Option<String>,
    /// User name. Defaults to $GLKVM_USER, then "admin".
    #[arg(long, global = true)]
    user: Option<String>,
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
    let creds = Credentials { user, password };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("glkvm-io")
        .enable_all()
        .build()?;
    let pins = Arc::new(PinStore::load_default()?);
    let session = rt
        .block_on(Session::login(&host, &creds, pins))
        .context("login")?;
    tracing::info!(host = %session.host(), "logged in");

    match cli.cmd {
        Cmd::View {
            swap_cmd_ctrl,
            iso_fix,
            scroll_rate,
            codec,
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
                codec,
                render,
                bench.map(Duration::from_secs),
            )
        }
        Cmd::Probe { frames } => rt.block_on(probe(session, frames)),
    }
}

fn view(
    rt: &tokio::runtime::Runtime,
    session: Session,
    (swap_cmd_ctrl, iso_fix, scroll_rate): (bool, Toggle, Option<u32>),
    codec: CodecArg,
    render: RenderPath,
    bench: Option<Duration>,
) -> Result<()> {
    let config = rt.block_on(session.system_config()).unwrap_or_else(|e| {
        tracing::warn!(%e, "cannot read device config, using defaults");
        Default::default()
    });
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
    let media = direct::start(session.clone(), DirectOptions { prefer });

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
    println!(
        "streamer (before /api/ws): {}",
        streamer
            .get("streamer")
            .map_or("?".into(), |s| s.to_string())
    );

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
        let text = ev.data.to_string();
        println!("event {:<12} {}", ev.kind, &text[..text.len().min(300)]);
    }
    let streamer = session.streamer_state().await?;
    println!("streamer (after /api/ws, 3 s): {streamer}");

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
    println!("streamer (while viewing): {streamer}");
    drop(media);
    events.close().await;
    Ok(())
}
