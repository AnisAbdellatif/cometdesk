//! Shared helpers for CLI subcommands: streamer keep-alive and interactive confirmation.
//!
//! Entry points: `with_streamer`, `confirm`.

use std::io::{IsTerminal, Write};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use cometdesk::Session;
use cometdesk::events::{ConnState, EventOptions, EventSession};

/// Keeps the streamer running while `f` runs (the device only captures with a viewer).
pub(super) async fn with_streamer<T>(
    session: &Session,
    f: impl AsyncFnOnce() -> Result<T>,
) -> Result<T> {
    let events = EventSession::spawn(session.clone(), EventOptions::default());
    let mut conn = events.conn_state();
    tokio::time::timeout(
        Duration::from_secs(5),
        conn.wait_for(|s| *s == ConnState::Connected),
    )
    .await
    .context("event socket did not connect")??;
    // Wait until the streamer reports a captured picture (cold start takes 1-2 s).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let state = session.streamer_state().await?;
        match state.streamer {
            Some(rt) if rt.has_signal() && rt.source.captured_fps > 0 => break,
            Some(rt) if !rt.hdmi.signal && tokio::time::Instant::now() > deadline => {
                bail!("no HDMI signal: the target is off or asleep")
            }
            _ if tokio::time::Instant::now() > deadline => bail!("streamer did not start"),
            _ => tokio::time::sleep(Duration::from_millis(250)).await,
        }
    }
    let result = f().await;
    events.close().await;
    result
}

/// Asks on the terminal unless `yes`.
pub(super) fn confirm(question: &str, yes: bool) -> Result<()> {
    if yes {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        bail!("{question}: pass --yes to confirm non-interactively");
    }
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    if !matches!(answer.trim(), "y" | "Y" | "yes") {
        bail!("cancelled");
    }
    Ok(())
}
