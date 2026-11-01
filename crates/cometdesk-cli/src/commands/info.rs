//! Device status summary and JPEG screen snapshot subcommands.
//!
//! Entry points: `info`, `snapshot`.

use std::path::Path;

use anyhow::{Context, Result};
use cometdesk::Session;
use cometdesk::api::capabilities::Feature;
use cometdesk::api::streamer::SnapshotOptions;

use super::util::with_streamer;

pub async fn info(session: &Session) -> Result<()> {
    let caps = session.capabilities().await?;
    let sys = session.system_info().await?;
    let hid = session.hid_state().await?;
    let streamer = session.streamer_state().await?;
    println!(
        "model      {} ({})",
        caps.firmware.model, caps.firmware.version
    );
    println!(
        "kvmd       {}  ustreamer {}",
        sys.kvmd.version, sys.streamer.version
    );
    println!("platform   {}", sys.platform.base);
    let features = [
        (Feature::Hid, "hid"),
        (Feature::RelativeMouse, "relative-mouse"),
        (Feature::Atx, "atx"),
        (Feature::Msd, "msd"),
        (Feature::WakeOnLan, "wol"),
        (Feature::H264, "h264"),
        (Feature::H265, "h265"),
    ];
    let supported: Vec<_> = features
        .iter()
        .filter(|(f, _)| caps.supports(*f))
        .map(|(_, n)| *n)
        .collect();
    println!("features   {}", supported.join(" "));
    println!(
        "target     usb {}  keyboard {}  mouse {}",
        on_off(hid.online),
        on_off(hid.keyboard.online),
        on_off(hid.mouse.online)
    );
    match streamer.streamer {
        Some(rt) => println!(
            "video      {}  {}x{} @ {} fps",
            if rt.has_signal() {
                "signal"
            } else {
                "no signal"
            },
            rt.source.resolution.width,
            rt.source.resolution.height,
            rt.source.captured_fps
        ),
        None => println!("video      streamer idle (no viewer)"),
    }
    Ok(())
}

fn on_off(b: bool) -> &'static str {
    if b { "online" } else { "offline" }
}

pub async fn snapshot(session: &Session, out: &Path, preview: Option<(u32, u32)>) -> Result<()> {
    let jpeg = with_streamer(session, async || {
        Ok(session
            .snapshot(SnapshotOptions {
                allow_offline: false,
                preview,
            })
            .await?)
    })
    .await?;
    std::fs::write(out, &jpeg).with_context(|| format!("write {}", out.display()))?;
    println!("wrote {} ({} bytes)", out.display(), jpeg.len());
    Ok(())
}
