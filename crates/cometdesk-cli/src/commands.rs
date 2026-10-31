//! One-shot commands: device info, snapshot, power, Wake-on-LAN, typing, fixture recording.

use std::io::{IsTerminal, Read, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use cometdesk::Session;
use cometdesk::api::atx::{AtxButton, PowerAction};
use cometdesk::api::bridge;
use cometdesk::api::capabilities::Feature;
use cometdesk::api::hid::PrintOptions;
use cometdesk::api::streamer::{QualityPreset, SnapshotOptions, StreamerUpdate};
use cometdesk::api::system::{LatencyMode, Orientation, VencMode, VideoFormat};
use cometdesk::api::wol::Mac;
use cometdesk::events::{ConnState, EventOptions, EventSession};
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
    mode: Option<cometdesk::api::system::VideoMode>,
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

pub async fn venc_mode(session: &Session, mode: Option<VencMode>, yes: bool) -> Result<()> {
    let current = session.system_config().await?.venc_mode;
    println!("venc_mode: {}", current.as_deref().unwrap_or("unknown"));
    let Some(mode) = mode else { return Ok(()) };
    if current.as_deref() == Some(mode.wire_name()) {
        println!("already {}", mode.wire_name());
        return Ok(());
    }
    confirm(
        &format!(
            "Switch the encoder to \"{}\" mode? This is device-wide and persisted.",
            mode.wire_name()
        ),
        yes,
    )?;
    session.set_venc_mode(mode).await?;
    println!("venc_mode: {} (confirmed)", mode.wire_name());
    Ok(())
}

pub async fn latency_mode(session: &Session, mode: Option<LatencyMode>, yes: bool) -> Result<()> {
    let current = session.system_config().await?.video_processing;
    println!(
        "video_processing: {}",
        current.as_deref().unwrap_or("unknown")
    );
    let Some(mode) = mode else { return Ok(()) };
    if current.as_deref() == Some(mode.wire_name()) {
        println!("already {}", mode.wire_name());
        return Ok(());
    }
    confirm(
        &format!(
            "Switch the latency mode to \"{}\"? This is device-wide and persisted.",
            mode.wire_name()
        ),
        yes,
    )?;
    session.set_latency_mode(mode).await?;
    println!("video_processing: {} (confirmed)", mode.wire_name());
    Ok(())
}

pub async fn codec(session: &Session, format: Option<VideoFormat>, yes: bool) -> Result<()> {
    let current = session.video_format().await?;
    let st = session.streamer_state().await.ok();
    let h265_supported = st.as_ref().map(|s| s.features.h265).unwrap_or(false);
    println!(
        "codec: {} (H.265 supported: {})",
        current.map_or("unknown", |f| f.name()),
        if h265_supported { "yes" } else { "no" },
    );
    let Some(format) = format else { return Ok(()) };
    if current == Some(format) {
        println!("already {}", format.name());
        return Ok(());
    }
    if format == VideoFormat::H265 && !h265_supported {
        bail!("device does not support H.265 (features.h265 is false)");
    }
    confirm(
        &format!(
            "Switch the encoder codec to \"{}\"? This is device-wide and persisted: \
             other viewers are affected and the encoder restarts.",
            format.name(),
        ),
        yes,
    )?;
    session.set_video_format(format).await?;
    println!("codec: {} (confirmed)", format.name());
    Ok(())
}

pub async fn orientation(session: &Session, o: Option<Orientation>, yes: bool) -> Result<()> {
    let current = session.system_config().await?.orientation;
    println!(
        "orientation: {}",
        current.map_or("unknown".into(), |d| d.to_string())
    );
    let Some(o) = o else { return Ok(()) };
    if current == Some(o.degrees()) {
        println!("already {}", o.degrees());
        return Ok(());
    }
    confirm(
        &format!(
            "Rotate the picture to {}°? This is device-wide and persisted: other viewers \
             (web UI, other apps) are affected.",
            o.degrees()
        ),
        yes,
    )?;
    session.set_orientation(o).await?;
    println!("orientation: {} (confirmed)", o.degrees());
    Ok(())
}

pub async fn edid(session: &Session, key: Option<&str>, yes: bool) -> Result<()> {
    let current = session.edid().await?;
    let presets = session.edid_presets().await?;
    let is_current = |p: &cometdesk::api::edid::EdidPreset| {
        cometdesk::api::edid::normalize(&p.content) == current
    };
    let Some(key) = key else {
        for p in &presets {
            println!(
                "{} {:<18} {}{}",
                if is_current(p) { "*" } else { " " },
                p.key,
                p.label,
                if p.audio { "" } else { " (no audio)" }
            );
        }
        if !presets.iter().any(is_current) {
            println!("* (custom EDID)");
        }
        return Ok(());
    };
    let Some(p) = presets.iter().find(|p| p.key == key) else {
        bail!("unknown EDID preset {key:?}; run `edid` to list them");
    };
    if is_current(p) {
        println!("already {}", p.label);
        return Ok(());
    }
    confirm(
        &format!(
            "Present the target a {} display? The target sees a monitor change. \
             This is device-wide and persisted.",
            p.label
        ),
        yes,
    )?;
    session.set_edid(&p.content).await?;
    println!("edid: {} (confirmed)", p.label);
    Ok(())
}

pub async fn quality(
    session: &Session,
    preset: Option<QualityPreset>,
    manual: StreamerUpdate,
    yes: bool,
) -> Result<()> {
    let st = session.streamer_state().await?;
    let show = |st: &cometdesk::api::streamer::StreamerState| {
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
    let mut update = StreamerUpdate {
        h264_bitrate: preset.map(|p| p.bitrate(&st.limits)),
        ..Default::default()
    };
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

async fn require_msd(session: &Session) -> Result<cometdesk::api::msd::MsdState> {
    let state = session.msd_state().await?;
    if !state.enabled {
        bail!("virtual media is not enabled on this device");
    }
    Ok(state)
}

fn mib(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / (1 << 20) as f64)
}

pub async fn msd_status(session: &Session) -> Result<()> {
    let s = require_msd(session).await?;
    match (s.mounted(), &s.drive.image) {
        (Some(img), _) => println!(
            "mounted    {img} as {}",
            if s.drive.cdrom { "CD-ROM" } else { "USB drive" }
        ),
        (None, Some(img)) => println!("selected   {img} (not connected)"),
        (None, None) => println!("mounted    nothing"),
    }
    if let Some(free) = s.free() {
        println!("free       {}", mib(free));
    }
    for (name, img) in &s.storage.images {
        let note = if img.complete {
            ""
        } else {
            "  (incomplete upload)"
        };
        println!("{name:40} {:>12}{note}", mib(img.size));
    }
    Ok(())
}

pub async fn msd_upload(session: &Session, file: &Path, name: Option<String>) -> Result<()> {
    let state = require_msd(session).await?;
    let name = match name {
        Some(n) => n,
        None => file
            .file_name()
            .and_then(|n| n.to_str())
            .context("the file name isn't valid UTF-8; pass --name")?
            .to_owned(),
    };
    let size = std::fs::metadata(file)
        .with_context(|| format!("stat {}", file.display()))?
        .len();
    if state.storage.images.contains_key(&name) {
        bail!("an image named {name} already exists; remove it first or pass --name");
    }
    if state.free().is_some_and(|free| size > free) {
        bail!(
            "{} doesn't fit: {} free",
            mib(size),
            mib(state.free().unwrap())
        );
    }
    let last = std::sync::atomic::AtomicU64::new(0);
    session
        .msd_upload(file, &name, move |sent, total| {
            let pct = sent * 100 / total.max(1);
            if pct != last.swap(pct, std::sync::atomic::Ordering::Relaxed) {
                eprint!("\ruploading: {pct}%");
            }
        })
        .await?;
    eprintln!();
    println!("uploaded {name} ({})", mib(size));
    Ok(())
}

pub async fn msd_mount(session: &Session, image: &str, cdrom: bool, yes: bool) -> Result<()> {
    let state = require_msd(session).await?;
    if !state.storage.images.get(image).is_some_and(|i| i.complete) {
        bail!("no complete image named {image}; see `msd status`");
    }
    if state.drive.connected {
        bail!("a drive is already connected; run `msd unmount` first");
    }
    confirm(
        &format!(
            "Connect {image} to the target as a {}? (device-wide)",
            if cdrom { "CD-ROM" } else { "USB drive" }
        ),
        yes,
    )?;
    session.msd_mount(image, cdrom).await?;
    println!("mounted {image}");
    Ok(())
}

pub async fn msd_unmount(session: &Session, yes: bool) -> Result<()> {
    let state = require_msd(session).await?;
    if !state.drive.connected {
        println!("nothing is mounted");
        return Ok(());
    }
    confirm("Disconnect the drive from the target? (device-wide)", yes)?;
    session.msd_set_connected(false).await?;
    println!("unmounted");
    Ok(())
}

pub async fn msd_remove(session: &Session, image: &str, yes: bool) -> Result<()> {
    let state = require_msd(session).await?;
    if !state.storage.images.contains_key(image) {
        bail!("no image named {image}; see `msd status`");
    }
    if state.mounted() == Some(image) {
        bail!("{image} is mounted; run `msd unmount` first");
    }
    confirm(&format!("Delete {image} from the device?"), yes)?;
    session.msd_remove(image).await?;
    println!("removed {image}");
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
/// them are GETs on this firmware (see `cometdesk::api`).
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
        println!("The bridge is not installed. Run: cometdesk bridge setup");
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
