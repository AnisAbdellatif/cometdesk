//! One-shot commands: device info, snapshot, power, Wake-on-LAN, typing, fixture recording.

use std::io::{IsTerminal, Read, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use glkvm::Session;
use glkvm::api::atx::{AtxButton, PowerAction};
use glkvm::api::bridge;
use glkvm::api::capabilities::Feature;
use glkvm::api::hid::PrintOptions;
use glkvm::api::streamer::{QualityPreset, SnapshotOptions, StreamerUpdate};
use glkvm::api::wol::Mac;
use glkvm::events::{ConnState, EventOptions, EventSession};
use serde_json::{Value, json};

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

/// Keeps the streamer running while `f` runs (the device only captures with a viewer).
async fn with_streamer<T>(session: &Session, f: impl AsyncFnOnce() -> Result<T>) -> Result<T> {
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

/// Asks on the terminal unless `yes`.
fn confirm(question: &str, yes: bool) -> Result<()> {
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

pub async fn power(session: &Session, action: PowerAction, yes: bool) -> Result<()> {
    require_atx(session).await?;
    confirm(
        &format!(
            "Send ATX power action \"{}\" to the target?",
            action.as_str()
        ),
        yes,
    )?;
    session.atx_power(action, true).await?;
    println!("done; power LED: {:?}", session.atx_state().await?.power);
    Ok(())
}

pub async fn video_mode(
    session: &Session,
    mode: Option<glkvm::api::system::VideoMode>,
    yes: bool,
) -> Result<()> {
    let current = session.system_config().await?.video_mode;
    println!("video_mode: {}", current.as_deref().unwrap_or("unknown"));
    let Some(mode) = mode else { return Ok(()) };
    if current.as_deref() == Some(mode.wire_name()) {
        println!("already {}", mode.wire_name());
        return Ok(());
    }
    confirm(
        &format!(
            "Switch the device's video mode to \"{}\"? This is device-wide and persisted: \
             other viewers (web UI, other apps) are affected.",
            mode.wire_name()
        ),
        yes,
    )?;
    session.set_video_mode(mode).await?;
    println!("video_mode: {} (confirmed)", mode.wire_name());
    Ok(())
}

pub async fn quality(
    session: &Session,
    preset: Option<QualityPreset>,
    manual: StreamerUpdate,
    yes: bool,
) -> Result<()> {
    let st = session.streamer_state().await?;
    let show = |st: &glkvm::api::streamer::StreamerState| {
        let (p, l) = (&st.params, &st.limits);
        println!(
            "fps {:?} ({}-{}), bitrate {:?} kbit/s ({}-{}), gop {:?} ({}-{}), quality {:?}",
            p.desired_fps,
            l.desired_fps.min,
            l.desired_fps.max,
            p.h264_bitrate,
            l.h264_bitrate.min,
            l.h264_bitrate.max,
            p.h264_gop,
            l.h264_gop.min,
            l.h264_gop.max,
            p.quality
        );
    };
    show(&st);
    let mut update = preset.map(|p| p.update(&st.limits)).unwrap_or_default();
    for (dst, src) in [
        (&mut update.desired_fps, manual.desired_fps),
        (&mut update.h264_bitrate, manual.h264_bitrate),
        (&mut update.h264_gop, manual.h264_gop),
        (&mut update.quality, manual.quality),
    ] {
        *dst = src.or(*dst);
    }
    if update == StreamerUpdate::default() {
        return Ok(());
    }
    let update = update.clamped(&st.limits);
    confirm(
        &format!(
            "Change the encoder to {update:?}? This is device-wide and persisted: every viewer is affected."
        ),
        yes,
    )?;
    show(&session.set_streamer_params(update).await?);
    Ok(())
}

pub async fn click(session: &Session, button: AtxButton, yes: bool) -> Result<()> {
    require_atx(session).await?;
    confirm(
        &format!("Press the ATX \"{}\" button?", button.as_str()),
        yes,
    )?;
    session.atx_click(button, true).await?;
    println!("done");
    Ok(())
}

async fn require_atx(session: &Session) -> Result<()> {
    if !session.atx_state().await?.enabled {
        bail!("ATX is not enabled on this device (no ATX board configured)");
    }
    Ok(())
}

/// Wakes a MAC address or a saved target by name.
pub async fn wake(session: &Session, target: &str) -> Result<()> {
    let mac = match target.parse::<Mac>() {
        Ok(mac) => mac,
        Err(_) => {
            let devices = session.wol_list().await?;
            let found = devices
                .iter()
                .find(|d| d.name.eq_ignore_ascii_case(target))
                .with_context(|| {
                    format!("{target:?} is neither a MAC address nor a saved target")
                })?;
            found.mac.parse()?
        }
    };
    session.wol_wake(mac).await?;
    println!("magic packet sent to {mac}");
    Ok(())
}

pub async fn wol_list(session: &Session) -> Result<()> {
    let devices = session.wol_list().await?;
    if devices.is_empty() {
        println!("no saved targets");
    }
    for d in devices {
        println!("{:<20} {:<17} {}", d.name, d.mac, d.ip);
    }
    Ok(())
}

pub async fn type_text(
    session: &Session,
    text: Option<String>,
    keymap: Option<&str>,
    slow: bool,
) -> Result<()> {
    let text = match text {
        Some(t) => t,
        None => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf
        }
    };
    if text.is_empty() {
        bail!("nothing to type");
    }
    session
        .hid_print(&text, PrintOptions { keymap, slow })
        .await?;
    println!("typed {} characters", text.chars().count());
    Ok(())
}

/// Read-only routes recorded as fixtures. Never add state-changing routes here: some of
/// them are GETs on this firmware (see `glkvm::api`).
/// (fixture name, route, query)
type Route = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const FIXTURE_ROUTES: &[Route] = &[
    ("info", "info", &[]),
    ("upgrade_version", "upgrade/version", &[]),
    ("atx", "atx", &[]),
    ("wol_list", "wol/list", &[]),
    ("hid", "hid", &[]),
    ("hid_keymaps", "hid/keymaps", &[]),
    ("msd", "msd", &[]),
    ("system_capability", "system/capability", &[]),
    ("system_get_config", "system/get_config", &[]),
    ("2fa_is_enabled", "2fa/is_enabled", &[]),
];

/// Keys whose values identify a person or device, or are secrets.
const SCRUB_KEYS: &[&str] = &[
    "serial",
    "uuid",
    "mac",
    "token",
    "password",
    "username",
    "secret",
    "sn",
    "device_id",
    "ddns",
];

/// Replaces sensitive values in place; renames MSD image files.
pub fn scrub(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                if SCRUB_KEYS.contains(&k.as_str()) && !val.is_null() {
                    *val = json!("<scrubbed>");
                } else {
                    scrub(val);
                }
            }
            if let Some(Value::Object(images)) = map.get_mut("images") {
                let renamed = std::mem::take(images)
                    .into_iter()
                    .enumerate()
                    .map(|(i, (_, meta))| (format!("image-{i}.iso"), meta))
                    .collect();
                *images = renamed;
            }
        }
        Value::Array(items) => items.iter_mut().for_each(scrub),
        _ => {}
    }
}

pub async fn record_fixtures(session: &Session, dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let write = |name: &str, result: Value| -> Result<()> {
        let mut doc = json!({"ok": true, "result": result});
        scrub(&mut doc);
        let path = dir.join(format!("{name}.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&doc)? + "\n")?;
        println!("recorded {}", path.display());
        Ok(())
    };
    for (name, route, query) in FIXTURE_ROUTES {
        write(name, session.get::<Value>(route, query).await?)?;
    }
    write(
        "streamer_idle",
        session.get::<Value>("streamer", &[]).await?,
    )?;
    let running = with_streamer(session, async || {
        Ok(session.get::<Value>("streamer", &[]).await?)
    })
    .await?;
    write("streamer_running", running)?;
    Ok(())
}

pub async fn bridge_status(session: &Session) -> Result<()> {
    let status = session.bridge_status().await?;
    println!("health:   {:?}", bridge::health(status.as_ref()));
    let Some(st) = status else {
        println!("The bridge is not installed. Run: glkvm bridge setup");
        return Ok(());
    };
    println!(
        "relay:    {} (this build installs {})",
        st.version,
        bridge::BRIDGE_VERSION
    );
    println!("firmware: {}", st.firmware.as_deref().unwrap_or("?"));
    if let Some(u) = &st.usb {
        println!(
            "usb:      gadget {} / interface {} / target link {}",
            yes_no(u.gadget),
            yes_no(u.iface),
            yes_no(u.carrier)
        );
    }
    for a in &st.agents {
        println!("agent:    {} ({} bytes, {:.12})", a.name, a.size, a.sha256);
    }
    println!(
        "session:  agent {}{}, client {}",
        if st.agent.connected {
            "connected"
        } else {
            "absent"
        },
        st.agent
            .peer
            .as_deref()
            .map(|p| format!(" ({p})"))
            .unwrap_or_default(),
        if st.client.connected {
            "connected"
        } else {
            "absent"
        },
    );
    Ok(())
}

fn yes_no(b: bool) -> &'static str {
    if b { "yes" } else { "NO" }
}

pub async fn bridge_setup(session: &Session, agents: &Path) -> Result<()> {
    let before = session.bridge_status().await?;
    println!("health before: {:?}", bridge::health(before.as_ref()));
    let report = bridge::setup(session, |msg| eprintln!("  {msg}")).await?;
    for c in &report.changes {
        println!("  {c}");
    }
    println!("relay {} installed", report.version);
    if report.usb_restarted {
        println!("(the target's USB devices were re-created: keyboard and mouse dropped briefly)");
    }
    let status = session
        .bridge_status()
        .await?
        .context("the bridge does not answer after setup")?;
    let uploaded = bridge::sync_agents(session, agents, &status).await?;
    if uploaded.is_empty() {
        println!("agent binaries: up to date");
    } else {
        println!("agent binaries uploaded: {}", uploaded.join(", "));
    }
    Ok(())
}

pub async fn bridge_remove(session: &Session, yes: bool) -> Result<()> {
    if !yes {
        bail!("this removes the bridge and restarts the device's USB functions; pass --yes");
    }
    bridge::remove(session).await?;
    println!("bridge removed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubs_secrets_and_identifiers() {
        let mut v = json!({
            "system": {"platform": {"serial": "E7142E", "model": "v3"}},
            "turn": {"username": "u", "password": "p", "uris": ["turn:x"]},
            "devices": [{"mac": "aa:bb:cc:dd:ee:ff", "name": "nas"}],
            "available_devices": {"/dev/x": {"uuid": "005D-9024"}},
            "storage": {"images": {"my-secret-project.iso": {"size": 1}}},
            "drive": {"image": null},
        });
        scrub(&mut v);
        let s = v.to_string();
        for secret in [
            "E7142E",
            "\"u\"",
            "\"p\"",
            "aa:bb",
            "005D",
            "my-secret-project",
        ] {
            assert!(!s.contains(secret), "{secret} leaked: {s}");
        }
        assert_eq!(v["system"]["platform"]["model"], "v3");
        assert_eq!(v["storage"]["images"]["image-0.iso"]["size"], 1);
        assert!(v["drive"]["image"].is_null(), "null values stay null");
    }

    #[test]
    fn fixture_routes_are_read_only_paths() {
        // Guard against someone adding a mutating GET.
        let forbidden = [
            "reboot",
            "reset",
            "delete",
            "create",
            "partition",
            "set_",
            "remove",
            "wake",
            "print",
        ];
        for (_, route, _) in FIXTURE_ROUTES {
            assert!(!forbidden.iter().any(|f| route.contains(f)), "{route}");
        }
    }
}
