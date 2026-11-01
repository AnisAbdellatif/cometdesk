//! Windows target probing: Win32 keyboard layouts, input desktop lock state, IME status,
//! foreground process focus, battery (`GetSystemPowerStatus`), and system load.

use super::*;
use cometdesk_clip::{Battery, Load, SessionState};
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
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
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::Ime::ImmGetDefaultIMEWnd;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, GetKeyboardLayoutList};
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
