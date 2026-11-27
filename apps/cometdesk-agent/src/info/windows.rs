//! Windows target probing: IME status, battery (`GetSystemPowerStatus`) and system load here;
//! keyboard layouts, lock state, monitors and the focused process come from `desktop-kit`.

use super::*;
use cometdesk_clip::{Battery, Load, SessionState};
use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::System::SystemInformation::{
    GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows_sys::Win32::System::Threading::GetSystemTimes;
use windows_sys::Win32::UI::Input::Ime::ImmGetDefaultIMEWnd;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SendMessageW};

const WM_IME_CONTROL: u32 = 0x0283;
const IMC_GETOPENSTATUS: usize = 5;

pub fn invalidate() {}

/// Asks the foreground window to switch, like the language bar does.
pub fn set_layout(layout: &str) -> Result<(), String> {
    desktop_kit::keyboard::set_layout(layout).map_err(|e| e.to_string())
}

pub fn read() -> Info {
    // Without this Windows reports scaled-down coordinates to us.
    desktop_kit::display::set_dpi_aware();
    let langid = |hkl: u32| format!("win:{:04x}", hkl & 0xffff);
    let mut layouts: Vec<String> = desktop_kit::keyboard::layouts()
        .into_iter()
        .map(langid)
        .collect();
    layouts.dedup();
    Info {
        keyboard: Some(langid(desktop_kit::keyboard::foreground_hkl())),
        layouts,
        os_version: windows_version(),
        session: session(),
        displays: crate::display::list(100),
        ime: ime(),
        focus: focus(),
        user: std::env::var("USERNAME").ok(),
        uptime_secs: Some(unsafe { GetTickCount64() } / 1000),
        battery: battery(),
        load: Some(load()),
        volume: crate::volume::read(),
    }
}

fn load() -> Load {
    let ticks = |f: FILETIME| (f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64;
    static PREV: std::sync::Mutex<Option<(u64, u64)>> = std::sync::Mutex::new(None);
    let cpu = unsafe {
        let (mut idle, mut kernel, mut user): (FILETIME, FILETIME, FILETIME) = std::mem::zeroed();
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

fn session() -> SessionState {
    match desktop_kit::session::state() {
        desktop_kit::session::SessionState::Unknown => SessionState::Unknown,
        desktop_kit::session::SessionState::Active => SessionState::Active,
        desktop_kit::session::SessionState::Locked => SessionState::Locked,
    }
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
    let app = desktop_kit::window::foreground()?.process;
    (!app.is_empty()).then(|| Focus {
        terminal: is_terminal(&app),
        app,
    })
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
