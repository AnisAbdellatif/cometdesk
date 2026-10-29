//! The target's monitors: what `Info::displays` reports, and changing a monitor's resolution
//! or making it the primary one. A display is identified by its index in `Info::displays`.

use cometdesk_clip::Display;

/// Modes beyond this many are dropped so the info stays small.
#[cfg(not(target_os = "macos"))]
const MAX_MODES: usize = 40;

#[cfg(not(target_os = "macos"))]
/// Sorted largest first, without repeats.
fn tidy(mut modes: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    modes.sort_by_key(|&(w, h)| std::cmp::Reverse((u64::from(w) * u64::from(h), w)));
    modes.dedup();
    modes.truncate(MAX_MODES);
    modes
}

/// The monitors in the order `Info::displays` has them. `scale` is the desktop's scale in
/// percent where the OS gives no per-monitor value.
pub fn list(scale: u32) -> Vec<Display> {
    sys::list(scale).into_iter().map(|m| m.display).collect()
}

/// Changes the resolution of display `index` to `mode`, and/or makes it the primary one.
pub fn set(index: u32, mode: Option<(u32, u32)>, primary: bool) -> Result<(), String> {
    let monitors = sys::list(100);
    let m = monitors
        .get(index as usize)
        .ok_or("that display does not exist")?;
    if let Some(mode) = mode
        && !m.display.modes.contains(&mode)
    {
        return Err(format!("the display has no {}x{} mode", mode.0, mode.1));
    }
    if mode.is_none() && !primary {
        return Err("nothing to change".into());
    }
    sys::set(&monitors, index as usize, mode, primary)
}

// ----- Linux: xrandr (X11 and XWayland)

#[cfg(all(unix, not(target_os = "macos")))]
mod sys {
    use std::process::Command;

    use super::*;

    pub struct Monitor {
        /// The xrandr output name (`DP-1`).
        pub name: String,
        pub display: Display,
    }

    /// Connected outputs that have a geometry (`2560x1440+0+0`), with their modes.
    pub fn list(scale: u32) -> Vec<Monitor> {
        let Ok(out) = Command::new("xrandr").arg("--query").output() else {
            return Vec::new();
        };
        parse(&String::from_utf8_lossy(&out.stdout), scale)
    }

    pub(super) fn parse(text: &str, scale: u32) -> Vec<Monitor> {
        let mut monitors: Vec<Monitor> = Vec::new();
        // Whether the output being read is one we keep (its mode lines follow it).
        let mut keep = false;
        for line in text.lines() {
            if !line.starts_with([' ', '\t']) {
                keep = false;
                if !line.contains(" connected") {
                    continue;
                }
                let geom = line.split_whitespace().find(|t| {
                    t.contains('x')
                        && t.matches('+').count() == 2
                        && t.as_bytes()[0].is_ascii_digit()
                });
                let parsed = (|| {
                    let (size, pos) = geom?.split_once('+')?;
                    let (w, h) = size.split_once('x')?;
                    let (x, y) = pos.split_once('+')?;
                    Some(Display {
                        x: x.parse().ok()?,
                        y: y.parse().ok()?,
                        width: w.parse().ok()?,
                        height: h.parse().ok()?,
                        scale,
                        primary: line.contains(" primary"),
                        modes: Vec::new(),
                    })
                })();
                if let (Some(display), Some(name)) = (parsed, line.split_whitespace().next()) {
                    monitors.push(Monitor {
                        name: name.to_owned(),
                        display,
                    });
                    keep = true;
                }
            } else if keep
                && let Some(m) = monitors.last_mut()
                && let Some(tok) = line.split_whitespace().next()
                && let Some((w, h)) = tok.split_once('x')
                && let (Ok(w), Ok(h)) = (w.parse(), h.trim_end_matches('i').parse())
            {
                m.display.modes.push((w, h));
            }
        }
        for m in &mut monitors {
            m.display.modes = tidy(std::mem::take(&mut m.display.modes));
        }
        monitors
    }

    pub fn set(
        monitors: &[Monitor],
        index: usize,
        mode: Option<(u32, u32)>,
        primary: bool,
    ) -> Result<(), String> {
        let mut args = vec!["--output".to_owned(), monitors[index].name.clone()];
        if let Some((w, h)) = mode {
            args.extend(["--mode".to_owned(), format!("{w}x{h}")]);
        }
        if primary {
            args.push("--primary".into());
        }
        let out = Command::new("xrandr")
            .args(&args)
            .output()
            .map_err(|e| format!("cannot run xrandr: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn xrandr_output_gives_geometry_and_modes() {
            let text = "Screen 0: minimum 320 x 200, current 4480 x 1440, maximum 16384 x 16384
DP-1 connected primary 2560x1440+0+0 (normal left) 597mm x 336mm
   2560x1440     59.95*+
   1920x1080     60.00 +  50.00
   1920x1080i    60.00
eDP-1 connected 1920x1080+2560+0 (normal left) 344mm x 193mm
   1920x1080     60.00*+
   1280x720      60.00
HDMI-1 disconnected (normal left)
DP-2 connected (normal left)
   1024x768      60.00
";
            let m = parse(text, 100);
            assert_eq!(m.len(), 2);
            assert_eq!((m[0].name.as_str(), m[0].display.primary), ("DP-1", true));
            assert_eq!(m[0].display.modes, [(2560, 1440), (1920, 1080)]);
            assert_eq!((m[1].display.x, m[1].display.modes.len()), (2560, 2));
        }
    }
}

// ----- Windows: GDI

#[cfg(windows)]
mod sys {
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        CDS_NORESET, CDS_SET_PRIMARY, CDS_UPDATEREGISTRY, ChangeDisplaySettingsExW, DEVMODEW,
        DISP_CHANGE_SUCCESSFUL, DM_PELSHEIGHT, DM_PELSWIDTH, DM_POSITION, ENUM_CURRENT_SETTINGS,
        EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
    };
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

    use super::*;

    pub struct Monitor {
        /// `\\.\DISPLAY1`, NUL-terminated.
        device: Vec<u16>,
        pub display: Display,
    }

    pub fn list(_scale: u32) -> Vec<Monitor> {
        unsafe extern "system" fn each(m: HMONITOR, _: HDC, _: *mut RECT, out: LPARAM) -> i32 {
            let out = unsafe { &mut *(out as *mut Vec<Monitor>) };
            let mut mi: MONITORINFOEXW = unsafe { std::mem::zeroed() };
            mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if unsafe { GetMonitorInfoW(m, &mut mi as *mut _ as *mut _) } != 0 {
                let (mut dx, mut dy) = (96, 96);
                unsafe { GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) };
                let r = mi.monitorInfo.rcMonitor;
                let end = mi.szDevice.iter().position(|&c| c == 0).unwrap_or(0);
                let mut device = mi.szDevice[..end].to_vec();
                device.push(0);
                out.push(Monitor {
                    display: Display {
                        x: r.left,
                        y: r.top,
                        width: (r.right - r.left) as u32,
                        height: (r.bottom - r.top) as u32,
                        scale: dx * 100 / 96,
                        primary: mi.monitorInfo.dwFlags & 1 != 0,
                        modes: modes(&device),
                    },
                    device,
                });
            }
            1
        }
        let mut out: Vec<Monitor> = Vec::new();
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

    fn blank() -> DEVMODEW {
        let mut dm: DEVMODEW = unsafe { std::mem::zeroed() };
        dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        dm
    }

    fn modes(device: &[u16]) -> Vec<(u32, u32)> {
        let mut found = Vec::new();
        let mut dm = blank();
        for i in 0.. {
            if unsafe { EnumDisplaySettingsW(device.as_ptr(), i, &mut dm) } == 0 {
                break;
            }
            if dm.dmBitsPerPel >= 32 {
                found.push((dm.dmPelsWidth, dm.dmPelsHeight));
            }
        }
        tidy(found)
    }

    fn change(device: &[u16], dm: &DEVMODEW, flags: u32) -> Result<(), String> {
        let r = unsafe {
            ChangeDisplaySettingsExW(
                device.as_ptr(),
                dm,
                std::ptr::null_mut(),
                flags,
                std::ptr::null(),
            )
        };
        if r == DISP_CHANGE_SUCCESSFUL {
            Ok(())
        } else {
            Err(format!("Windows refused the change (code {r})"))
        }
    }

    pub fn set(
        monitors: &[Monitor],
        index: usize,
        mode: Option<(u32, u32)>,
        primary: bool,
    ) -> Result<(), String> {
        let target = &monitors[index];
        let mut dm = blank();
        if unsafe { EnumDisplaySettingsW(target.device.as_ptr(), ENUM_CURRENT_SETTINGS, &mut dm) }
            == 0
        {
            return Err("cannot read the display's current settings".into());
        }
        if let Some((w, h)) = mode {
            dm.dmPelsWidth = w;
            dm.dmPelsHeight = h;
            dm.dmFields |= DM_PELSWIDTH | DM_PELSHEIGHT;
        }
        if !primary || target.display.primary {
            return change(&target.device, &dm, CDS_UPDATEREGISTRY);
        }
        // The primary monitor sits at (0, 0): shift every other one by the same amount.
        let (px, py) = (target.display.x, target.display.y);
        for (i, m) in monitors.iter().enumerate().filter(|(i, _)| *i != index) {
            let mut other = blank();
            if unsafe { EnumDisplaySettingsW(m.device.as_ptr(), ENUM_CURRENT_SETTINGS, &mut other) }
                == 0
            {
                continue;
            }
            unsafe {
                let pos = &mut other.Anonymous1.Anonymous2.dmPosition;
                (pos.x, pos.y) = (m.display.x - px, m.display.y - py);
            }
            other.dmFields |= DM_POSITION;
            change(&m.device, &other, CDS_UPDATEREGISTRY | CDS_NORESET)
                .map_err(|e| format!("monitor {i}: {e}"))?;
        }
        unsafe {
            let pos = &mut dm.Anonymous1.Anonymous2.dmPosition;
            (pos.x, pos.y) = (0, 0);
        }
        dm.dmFields |= DM_POSITION;
        change(
            &target.device,
            &dm,
            CDS_UPDATEREGISTRY | CDS_NORESET | CDS_SET_PRIMARY,
        )?;
        // Apply everything that was queued.
        let r = unsafe {
            ChangeDisplaySettingsExW(
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        if r == DISP_CHANGE_SUCCESSFUL {
            Ok(())
        } else {
            Err(format!("Windows refused the change (code {r})"))
        }
    }
}

// ----- macOS: no tool to rely on

#[cfg(target_os = "macos")]
mod sys {
    use super::*;

    pub struct Monitor {
        pub display: Display,
    }

    pub fn list(_: u32) -> Vec<Monitor> {
        Vec::new()
    }

    pub fn set(_: &[Monitor], _: usize, _: Option<(u32, u32)>, _: bool) -> Result<(), String> {
        Err("changing displays is not supported on macOS targets".into())
    }
}
