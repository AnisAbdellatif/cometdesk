//! What the agent can tell the GUI about the target besides the clipboard: keyboard layouts,
//! session lock state, monitors, input method, focused application, account, uptime, battery,
//! OS version, and CPU, memory and disk use. Polled, because all of it changes while the agent
//! runs. Every field is best effort and `None` or empty where the OS offers no cheap way to ask.

use std::time::{Duration, Instant};

pub use cometdesk_clip::Info;
use cometdesk_clip::{Focus, Usage};

const EVERY: Duration = Duration::from_secs(2);
/// Load moves on every poll, so it is resent on this timer instead of on change.
const LOAD_EVERY: Duration = Duration::from_secs(10);

/// Yields the info on the first call, then when it changed or the load is due again.
pub struct Probe {
    last: Option<Info>,
    at: Option<Instant>,
    sent: Option<Instant>,
}

impl Probe {
    pub fn new() -> Probe {
        Probe {
            last: None,
            at: None,
            sent: None,
        }
    }

    /// Report again on the next call, e.g. after something the GUI asked for changed the target.
    pub fn refresh(&mut self) {
        self.at = None;
        self.sent = None;
        sys::invalidate();
    }

    pub fn changed(&mut self) -> Option<Info> {
        if self.at.is_some_and(|t| t.elapsed() < EVERY) {
            return None;
        }
        self.at = Some(Instant::now());
        let now = read();
        if self.last.as_ref().is_some_and(|l| l.same_as(&now))
            && self.sent.is_some_and(|t| t.elapsed() < LOAD_EVERY)
        {
            return None;
        }
        self.sent = Some(Instant::now());
        self.last = Some(now.clone());
        Some(now)
    }
}

pub fn read() -> Info {
    sys::read()
}

/// Makes `layout` (one of `Info::layouts`) the active keyboard layout.
pub fn set_layout(layout: &str) -> Result<(), String> {
    sys::set_layout(layout)
}

/// The focused application, for choosing the paste keystroke.
pub fn focus() -> Option<Focus> {
    sys::focus()
}

#[cfg(not(target_os = "macos"))]
const TERMINALS: &[&str] = &[
    "terminal",
    "konsole",
    "xterm",
    "kitty",
    "alacritty",
    "terminator",
    "tilix",
    "wezterm",
    "foot",
    "urxvt",
    "rxvt",
    "st-256color",
    "termite",
    "guake",
    "yakuake",
    "tilda",
    "ptyxis",
    "blackbox",
    "contour",
    "ghostty",
    "conhost",
    "powershell",
    "pwsh",
    "cmd.exe",
    "mintty",
];

#[cfg(not(target_os = "macos"))]
fn is_terminal(app: &str) -> bool {
    let app = app.to_lowercase();
    TERMINALS.iter().any(|t| app.contains(t))
}

/// Used and total space of the filesystem holding `/`, from POSIX `df`.
#[cfg(unix)]
fn root_disk() -> Option<Usage> {
    let out = run("df", &["-Pk", "/"])?;
    let mut f = out.lines().nth(1)?.split_whitespace().skip(1);
    let (total, used): (u64, u64) = (f.next()?.parse().ok()?, f.next()?.parse().ok()?);
    Some(Usage {
        used_mb: used / 1024,
        total_mb: total / 1024,
    })
}

#[cfg(unix)]
fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(cmd).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

// ----- Linux

#[cfg(all(unix, not(target_os = "macos")))]
mod sys {
    use std::sync::Mutex;

    use super::*;
    use cometdesk_clip::{Battery, Display, Load, SessionState};

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
            displays: displays(slow.scale),
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

    /// `xrandr` geometry (`2560x1440+0+0`); `scale` is GNOME's cached integer factor.
    fn displays(scale: u32) -> Vec<Display> {
        let Some(out) = run("xrandr", &["--query"]) else {
            return Vec::new();
        };
        out.lines()
            .filter(|l| l.contains(" connected"))
            .filter_map(|l| {
                let geom = l.split_whitespace().find(|t| {
                    t.contains('x')
                        && t.matches('+').count() == 2
                        && t.as_bytes()[0].is_ascii_digit()
                })?;
                let (size, pos) = geom.split_once('+')?;
                let (w, h) = size.split_once('x')?;
                let (x, y) = pos.split_once('+')?;
                Some(Display {
                    x: x.parse().ok()?,
                    y: y.parse().ok()?,
                    width: w.parse().ok()?,
                    height: h.parse().ok()?,
                    scale,
                    primary: l.contains(" primary"),
                })
            })
            .collect()
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
}

// ----- macOS (a development convenience; targets are Windows and Linux)

#[cfg(target_os = "macos")]
mod sys {
    use super::*;
    use cometdesk_clip::Load;

    pub fn read() -> Info {
        let keyboard = run(
            "defaults",
            &[
                "read",
                "com.apple.HIToolbox",
                "AppleCurrentKeyboardLayoutInputSourceID",
            ],
        )
        .map(|o| format!("mac:{}", o.trim()));
        Info {
            layouts: keyboard.iter().cloned().collect(),
            keyboard,
            os_version: run("sw_vers", &["-productVersion"]).map(|v| format!("macOS {}", v.trim())),
            user: std::env::var("USER").ok(),
            load: Some(Load {
                disk: root_disk(),
                ..Load::default()
            }),
            ..Info::default()
        }
    }

    pub fn focus() -> Option<Focus> {
        None
    }

    pub fn invalidate() {}

    pub fn set_layout(_: &str) -> Result<(), String> {
        Err("switching layouts is not supported on macOS targets".into())
    }
}

// ----- Windows

#[cfg(windows)]
mod sys {
    use super::*;
    use cometdesk_clip::{Battery, Display, Load, SessionState};
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
    };
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, OpenInputDesktop};
    use windows_sys::Win32::System::SystemInformation::{
        GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX,
    };
    use windows_sys::Win32::System::Threading::{
        GetSystemTimes, OpenProcess, QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, MDT_EFFECTIVE_DPI,
        SetProcessDpiAwarenessContext,
    };
    use windows_sys::Win32::UI::Input::Ime::ImmGetDefaultIMEWnd;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, GetKeyboardLayoutList,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, PostMessageW, SendMessageW,
    };

    const DESKTOP_SWITCHDESKTOP: u32 = 0x0100;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const WM_IME_CONTROL: u32 = 0x0283;
    const IMC_GETOPENSTATUS: usize = 5;

    pub fn invalidate() {}

    /// Asks the foreground window to switch, like the language bar does.
    pub fn set_layout(layout: &str) -> Result<(), String> {
        let want = u16::from_str_radix(
            layout.strip_prefix("win:").ok_or("not a Windows layout")?,
            16,
        )
        .map_err(|e| e.to_string())?;
        let mut list = vec![std::ptr::null_mut(); 32];
        let n = unsafe { GetKeyboardLayoutList(list.len() as i32, list.as_mut_ptr()) }.max(0);
        let hkl = list[..n as usize]
            .iter()
            .find(|&&h| h as usize & 0xffff == want as usize)
            .ok_or("layout not installed")?;
        const WM_INPUTLANGCHANGEREQUEST: u32 = 0x0050;
        let ok = unsafe {
            PostMessageW(
                GetForegroundWindow(),
                WM_INPUTLANGCHANGEREQUEST,
                0,
                *hkl as isize,
            )
        };
        if ok != 0 {
            Ok(())
        } else {
            Err("no window to switch".into())
        }
    }

    pub fn read() -> Info {
        // Without this Windows reports scaled-down coordinates to us.
        static DPI: std::sync::Once = std::sync::Once::new();
        DPI.call_once(|| unsafe {
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        });
        let langid = |hkl: usize| format!("win:{:04x}", hkl & 0xffff);
        let foreground_thread =
            unsafe { GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut()) };
        let mut list = vec![std::ptr::null_mut(); 32];
        let n = unsafe { GetKeyboardLayoutList(list.len() as i32, list.as_mut_ptr()) }.max(0);
        let mut layouts: Vec<String> = list[..n as usize]
            .iter()
            .map(|&h| langid(h as usize))
            .collect();
        layouts.dedup();
        Info {
            keyboard: Some(langid(
                unsafe { GetKeyboardLayout(foreground_thread) } as usize
            )),
            layouts,
            os_version: windows_version(),
            session: session(),
            displays: displays(),
            ime: ime(),
            focus: focus(),
            user: std::env::var("USERNAME").ok(),
            uptime_secs: Some(unsafe { GetTickCount64() } / 1000),
            battery: battery(),
            load: Some(load()),
        }
    }

    fn load() -> Load {
        let ticks = |f: FILETIME| (f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64;
        static PREV: std::sync::Mutex<Option<(u64, u64)>> = std::sync::Mutex::new(None);
        let cpu = unsafe {
            let (mut idle, mut kernel, mut user): (FILETIME, FILETIME, FILETIME) =
                std::mem::zeroed();
            (GetSystemTimes(&mut idle, &mut kernel, &mut user) != 0)
                .then(|| {
                    // Kernel time includes idle time.
                    let (idle, total) = (ticks(idle), ticks(kernel) + ticks(user));
                    let prev = PREV.lock().unwrap().replace((idle, total))?;
                    let (di, dt) = (idle.saturating_sub(prev.0), total.saturating_sub(prev.1));
                    (dt > 0).then(|| (100 * (dt - di.min(dt)) / dt) as u8)
                })
                .flatten()
        };
        let memory = unsafe {
            let mut m: MEMORYSTATUSEX = std::mem::zeroed();
            m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            (GlobalMemoryStatusEx(&mut m) != 0).then(|| Usage {
                used_mb: (m.ullTotalPhys - m.ullAvailPhys) >> 20,
                total_mb: m.ullTotalPhys >> 20,
            })
        };
        let disk = unsafe {
            let root: Vec<u16> = "C:\\\0".encode_utf16().collect();
            let (mut free, mut total, mut _total_free) = (0u64, 0u64, 0u64);
            (GetDiskFreeSpaceExW(root.as_ptr(), &mut free, &mut total, &mut _total_free) != 0).then(
                || Usage {
                    used_mb: (total - free) >> 20,
                    total_mb: total >> 20,
                },
            )
        };
        Load {
            cpu_percent: cpu,
            memory,
            disk,
        }
    }

    /// The secure desktop (lock screen, UAC) is the input desktop, and we may not switch to
    /// it: opening it fails.
    fn session() -> SessionState {
        unsafe {
            let desk = OpenInputDesktop(0, 0, DESKTOP_SWITCHDESKTOP);
            if desk.is_null() {
                return SessionState::Locked;
            }
            CloseDesktop(desk);
        }
        SessionState::Active
    }

    fn displays() -> Vec<Display> {
        unsafe extern "system" fn each(m: HMONITOR, _: HDC, _: *mut RECT, out: LPARAM) -> i32 {
            let out = unsafe { &mut *(out as *mut Vec<Display>) };
            let mut mi: MONITORINFO = unsafe { std::mem::zeroed() };
            mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if unsafe { GetMonitorInfoW(m, &mut mi) } != 0 {
                let (mut dx, mut dy) = (96, 96);
                unsafe { GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) };
                let r = mi.rcMonitor;
                out.push(Display {
                    x: r.left,
                    y: r.top,
                    width: (r.right - r.left) as u32,
                    height: (r.bottom - r.top) as u32,
                    scale: dx * 100 / 96,
                    primary: mi.dwFlags & 1 != 0,
                });
            }
            1
        }
        let mut out: Vec<Display> = Vec::new();
        unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(each),
                &mut out as *mut _ as LPARAM,
            );
        }
        out
    }

    /// Whether the input method of the focused window is open (Japanese kana mode, …).
    fn ime() -> Option<bool> {
        unsafe {
            let wnd = ImmGetDefaultIMEWnd(GetForegroundWindow());
            if wnd.is_null() {
                return None;
            }
            Some(SendMessageW(wnd, WM_IME_CONTROL, IMC_GETOPENSTATUS, 0) != 0)
        }
    }

    pub fn focus() -> Option<Focus> {
        unsafe {
            let mut pid = 0u32;
            GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
            if pid == 0 {
                return None;
            }
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = [0u16; 520];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) != 0;
            CloseHandle(h);
            if !ok {
                return None;
            }
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            let app = path.rsplit('\\').next()?.to_owned();
            Some(Focus {
                terminal: is_terminal(&app),
                app,
            })
        }
    }

    fn battery() -> Option<Battery> {
        let mut s: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
        if unsafe { GetSystemPowerStatus(&mut s) } == 0
            || s.BatteryFlag == 128
            || s.BatteryLifePercent == 255
        {
            return None;
        }
        Some(Battery {
            percent: s.BatteryLifePercent,
            charging: s.ACLineStatus == 1,
        })
    }

    /// The first line of `cmd /c ver`, once.
    fn windows_version() -> Option<String> {
        use std::os::windows::process::CommandExt;
        static V: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
        V.get_or_init(|| {
            let out = std::process::Command::new("cmd")
                .args(["/c", "ver"])
                .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
                .output()
                .ok()?;
            let text = String::from_utf8_lossy(&out.stdout);
            text.lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .map(str::to_owned)
        })
        .clone()
    }
}
