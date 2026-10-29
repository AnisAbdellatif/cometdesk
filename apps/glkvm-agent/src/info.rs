//! What the agent can tell the GUI about the target besides the clipboard: keyboard layouts,
//! session lock state, monitors, input method, focused application, account, uptime, battery
//! and OS version. Polled, because all of it changes while the agent runs. Every field is
//! best effort and `None` or empty where the OS offers no cheap way to ask.

use std::time::{Duration, Instant};

use glkvm_clip::Focus;
pub use glkvm_clip::Info;

const EVERY: Duration = Duration::from_secs(2);

/// Yields the info on the first call, then only when it changed.
pub struct Probe {
    last: Option<Info>,
    at: Option<Instant>,
}

impl Probe {
    pub fn new() -> Probe {
        Probe {
            last: None,
            at: None,
        }
    }

    pub fn changed(&mut self) -> Option<Info> {
        if self.at.is_some_and(|t| t.elapsed() < EVERY) {
            return None;
        }
        self.at = Some(Instant::now());
        let now = read();
        if self.last.as_ref().is_some_and(|l| l.same_as(&now)) {
            return None;
        }
        self.last = Some(now.clone());
        Some(now)
    }
}

pub fn read() -> Info {
    sys::read()
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
    use super::*;
    use glkvm_clip::{Battery, Display, SessionState};

    pub fn read() -> Info {
        let (keyboard, layouts) = keyboards();
        Info {
            keyboard,
            layouts,
            os_version: std::fs::read_to_string("/etc/os-release")
                .ok()
                .and_then(|t| {
                    let v = t.lines().find_map(|l| l.strip_prefix("PRETTY_NAME="))?;
                    Some(v.trim_matches('"').to_owned())
                }),
            session: session(),
            displays: displays(),
            ime: ime(),
            focus: focus(),
            user: std::env::var("USER")
                .or_else(|_| std::env::var("LOGNAME"))
                .ok(),
            uptime_secs: std::fs::read_to_string("/proc/uptime")
                .ok()
                .and_then(|t| t.split('.').next()?.parse().ok()),
            battery: battery(),
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

    /// `xrandr` geometry (`2560x1440+0+0`); scale from GNOME's integer factor.
    fn displays() -> Vec<Display> {
        let Some(out) = run("xrandr", &["--query"]) else {
            return Vec::new();
        };
        let scale = run(
            "gsettings",
            &["get", "org.gnome.desktop.interface", "scaling-factor"],
        )
        .and_then(|t| t.split_whitespace().last()?.parse::<u32>().ok())
        .filter(|&s| s > 0)
        .map_or(100, |s| s * 100);
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
            ..Info::default()
        }
    }

    pub fn focus() -> Option<Focus> {
        None
    }
}

// ----- Windows

#[cfg(windows)]
mod sys {
    use super::*;
    use glkvm_clip::{Battery, Display, SessionState};
    use windows_sys::Win32::Foundation::{CloseHandle, LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
    };
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, OpenInputDesktop};
    use windows_sys::Win32::System::SystemInformation::GetTickCount64;
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW};
    use windows_sys::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, MDT_EFFECTIVE_DPI,
        SetProcessDpiAwarenessContext,
    };
    use windows_sys::Win32::UI::Input::Ime::ImmGetDefaultIMEWnd;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, GetKeyboardLayoutList,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, SendMessageW,
    };

    const DESKTOP_SWITCHDESKTOP: u32 = 0x0100;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const WM_IME_CONTROL: u32 = 0x0283;
    const IMC_GETOPENSTATUS: usize = 5;

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
