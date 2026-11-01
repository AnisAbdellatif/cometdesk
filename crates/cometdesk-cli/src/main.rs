//! `cometdesk`: command-line client for GLKVM devices.
//!
//! Credentials come from the environment or a `.env` file (values taken literally):
//! `COMETDESK_HOST`, `COMETDESK_USER` (default `admin`) and `COMETDESK_PASSWORD`. With 2FA enabled, pass
//! `--totp CODE` (or `COMETDESK_TOTP`), or type the code when asked.
//!
//! Entry point: `main`. See `args.rs` for CLI flags and `dispatch.rs` for subcommand routing.

mod args;
mod bench;
mod commands;
mod dispatch;
mod renderer;
mod viewer;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use cometdesk::Credentials;
use cometdesk::tls::PinStore;
use cometdesk_view::RenderPath;

use self::args::{Cli, Cmd, RenderArg};
use self::dispatch::{login, one_shot, probe, view};

fn main() -> Result<()> {
    cometdesk::env::load_dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let cli = Cli::parse();

    let host = cli
        .host
        .or_else(|| std::env::var("COMETDESK_HOST").ok())
        .context("no device: pass --host or set COMETDESK_HOST")?;
    let user = cli
        .user
        .or_else(|| std::env::var("COMETDESK_USER").ok())
        .unwrap_or_else(|| "admin".into());
    let password =
        std::env::var("COMETDESK_PASSWORD").context("set COMETDESK_PASSWORD (e.g. in .env)")?;
    let mut creds = Credentials::new(user, password);
    creds.totp = cli.totp.or_else(|| std::env::var("COMETDESK_TOTP").ok());

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("cometdesk-io")
        .on_thread_start(cometdesk_view::set_user_interactive_qos)
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
        Cmd::Probe {
            frames,
            key_request_at,
        } => rt.block_on(probe(session, frames, key_request_at)),
        cmd => rt.block_on(async {
            let result = one_shot(&session, cmd).await;
            let _ = session.logout().await;
            result
        }),
    }
}
