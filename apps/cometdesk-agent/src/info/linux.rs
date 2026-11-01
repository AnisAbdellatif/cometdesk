//! Linux target probing: keyboard layouts (`gsettings`/`setxkbmap`), session (`loginctl`),
//! IME (`ibus`/`fcitx5`), X11 focus (`_NET_ACTIVE_WINDOW`), battery, and `/proc` load.

use std::sync::Mutex;

use super::*;
use cometdesk_clip::{Battery, Load, SessionState, Volume};

/// Keyboard layout, input method and display scale rarely change and each costs one or more
/// subprocesses (`gsettings`, `setxkbmap`, `ibus`/`fcitx5-remote`); refresh them this often
/// instead of on every poll. The cheap fields (`loginctl`, `xrandr`, sysfs) stay live.
const SLOW: Duration = Duration::from_secs(15);

#[derive(Clone)]
struct Slow {
    keyboard: Option<String>,
    layouts: Vec<String>,
    ime: Option<bool>,
    scale: u32,
    volume: Option<Volume>,
}

/// Drops the cached slow fields so the next [`read`] asks the OS again.
pub fn invalidate() {
    *CACHE.lock().unwrap() = None;
}

static CACHE: Mutex<Option<(Instant, Slow)>> = Mutex::new(None);

/// The slow fields, recomputed at most every [`SLOW`].
fn slow() -> Slow {
    let mut cache = CACHE.lock().unwrap();
    if let Some((at, s)) = cache.as_ref()
        && at.elapsed() < SLOW
    {
        return s.clone();
    }
    let (keyboard, layouts) = keyboards();
    let s = Slow {
        keyboard,
        layouts,
        ime: ime(),
        scale: scale(),
        volume: crate::volume::read(),
    };
    *cache = Some((Instant::now(), s.clone()));
    s
}

pub fn read() -> Info {
    let slow = slow();
    Info {
        keyboard: slow.keyboard,
        layouts: slow.layouts,
        os_version: std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|t| {
                let v = t.lines().find_map(|l| l.strip_prefix("PRETTY_NAME="))?;
                Some(v.trim_matches('"').to_owned())
            }),
        session: session(),
        displays: crate::display::list(slow.scale),
        ime: slow.ime,
        focus: focus(),
        user: std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .ok(),
        uptime_secs: std::fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|t| t.split('.').next()?.parse().ok()),
        battery: battery(),
        load: Some(load()),
        volume: slow.volume,
    }
}

/// CPU from the delta of `/proc/stat` since the previous call, memory from `/proc/meminfo`.
fn load() -> Load {
    static PREV: Mutex<Option<(u64, u64)>> = Mutex::new(None);
    let cpu = std::fs::read_to_string("/proc/stat").ok().and_then(|t| {
        let v: Vec<u64> = t
            .lines()
            .next()?
            .split_whitespace()
            .skip(1)
            .filter_map(|n| n.parse().ok())
            .collect();
        // user nice system idle iowait irq softirq steal
        let idle = v.get(3)? + v.get(4).copied().unwrap_or(0);
        let total: u64 = v.iter().take(8).sum();
        let prev = PREV.lock().unwrap().replace((idle, total))?;
        let (di, dt) = (idle.saturating_sub(prev.0), total.saturating_sub(prev.1));
        (dt > 0).then(|| (100 * (dt - di.min(dt)) / dt) as u8)
    });
    let memory = std::fs::read_to_string("/proc/meminfo").ok().and_then(|t| {
        let kb = |key: &str| -> Option<u64> {
            t.lines().find_map(|l| {
                l.strip_prefix(key)?
                    .trim()
                    .strip_suffix("kB")?
                    .trim()
                    .parse()
                    .ok()
            })
        };
        let (total, avail) = (kb("MemTotal:")?, kb("MemAvailable:")?);
        Some(Usage {
            used_mb: total.saturating_sub(avail) / 1024,
            total_mb: total / 1024,
        })
    });
    Load {
        cpu_percent: cpu,
        memory,
        disk: root_disk(),
    }
}

/// Active layout first in a GNOME list; `setxkbmap` when GNOME is not asked or silent
/// (it reads the X server's, XWayland's on Wayland, which can be stale).
fn keyboards() -> (Option<String>, Vec<String>) {
    let gs = |key| {
        run(
            "gsettings",
            &["get", "org.gnome.desktop.input-sources", key],
        )
    };
    let ids = |text: &str| -> Vec<String> {
        text.split("'xkb', '")
            .skip(1)
            .filter_map(|r| Some(format!("xkb:{}", r.split('\'').next()?)))
            .collect()
    };
    let all = gs("sources").map(|t| ids(&t)).unwrap_or_default();
    let active = gs("mru-sources")
        .and_then(|t| ids(&t).into_iter().next())
        .or_else(|| all.first().cloned());
    if active.is_some() {
        return (active, all);
    }
    let Some(out) = run("setxkbmap", &["-query"]) else {
        return (None, Vec::new());
    };
    let list = |name: &str| -> Vec<String> {
        out.lines()
            .find_map(|l| l.strip_prefix(name))
            .map(|v| v.trim().split(',').map(str::to_owned).collect())
            .unwrap_or_default()
    };
    let (layouts, variants) = (list("layout:"), list("variant:"));
    let all: Vec<String> = layouts
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.is_empty())
        .map(|(i, l)| match variants.get(i).filter(|v| !v.is_empty()) {
            Some(v) => format!("xkb:{l}+{v}"),
            None => format!("xkb:{l}"),
        })
        .collect();
    (all.first().cloned(), all)
}

/// GNOME: select the matching source. Anything else: put the layout first in X's list.
pub fn set_layout(layout: &str) -> Result<(), String> {
    let spec = layout.strip_prefix("xkb:").ok_or("not an xkb layout")?;
    let src = "org.gnome.desktop.input-sources";
    if let Some(text) = run("gsettings", &["get", src, "sources"]) {
        let pos = text
            .split(['(', ')'])
            .filter(|e| e.contains('\''))
            .position(|e| e.contains(&format!("'{spec}'")));
        if let Some(i) = pos {
            return exec("gsettings", &["set", src, "current", &i.to_string()]);
        }
    }
    let (_, all) = keyboards();
    let mut order: Vec<&str> = all.iter().map(String::as_str).collect();
    let at = order
        .iter()
        .position(|l| *l == layout)
        .ok_or("unknown layout")?;
    order.swap(0, at);
    let (mut layouts, mut variants) = (Vec::new(), Vec::new());
    for l in order {
        let (code, v) = l
            .trim_start_matches("xkb:")
            .split_once('+')
            .unwrap_or((l.trim_start_matches("xkb:"), ""));
        layouts.push(code.to_owned());
        variants.push(v.to_owned());
    }
    exec(
        "setxkbmap",
        &[
            "-layout",
            &layouts.join(","),
            "-variant",
            &variants.join(","),
        ],
    )
}

fn exec(cmd: &str, args: &[&str]) -> Result<(), String> {
    let out = std::process::Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run {cmd}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

/// logind's `LockedHint`, which GNOME, KDE and others keep up to date.
fn session() -> SessionState {
    let id = std::env::var("XDG_SESSION_ID").unwrap_or_else(|_| "self".into());
    match run(
        "loginctl",
        &["show-session", &id, "-p", "LockedHint", "--value"],
    )
    .as_deref()
    .map(str::trim)
    {
        Some("yes") => SessionState::Locked,
        Some("no") => SessionState::Active,
        _ => SessionState::Unknown,
    }
}

/// GNOME's integer scaling factor as a percentage (100 = unscaled). A slow field.
fn scale() -> u32 {
    run(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "scaling-factor"],
    )
    .and_then(|t| t.split_whitespace().last()?.parse::<u32>().ok())
    .filter(|&s| s > 0)
    .map_or(100, |s| s * 100)
}

/// IBus: any engine but a plain `xkb:` layout is an input method. Fcitx5: 2 is active.
fn ime() -> Option<bool> {
    if let Some(e) = run("ibus", &["engine"]) {
        return Some(!e.trim().starts_with("xkb:"));
    }
    run("fcitx5-remote", &[]).map(|s| s.trim() == "2")
}

pub fn focus() -> Option<Focus> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots.get(screen)?.root;
    let active = conn
        .intern_atom(true, b"_NET_ACTIVE_WINDOW")
        .ok()?
        .reply()
        .ok()?
        .atom;
    let win = conn
        .get_property(false, root, active, AtomEnum::WINDOW, 0, 1)
        .ok()?
        .reply()
        .ok()?
        .value32()?
        .next()
        .filter(|&w| w != 0)?;
    // WM_CLASS is "instance\0class\0": the class names the application.
    let raw = conn
        .get_property(false, win, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)
        .ok()?
        .reply()
        .ok()?
        .value;
    let text = String::from_utf8_lossy(&raw);
    let app = text.split('\0').rfind(|s| !s.is_empty())?.to_owned();
    Some(Focus {
        terminal: is_terminal(&text),
        app,
    })
}

fn battery() -> Option<Battery> {
    let dir = std::fs::read_dir("/sys/class/power_supply")
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().starts_with("BAT"))?
        .path();
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok();
    Some(Battery {
        percent: read("capacity")?.trim().parse().ok()?,
        charging: read("status").is_some_and(|s| matches!(s.trim(), "Charging" | "Full")),
    })
}
