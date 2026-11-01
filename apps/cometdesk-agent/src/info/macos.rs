//! macOS target probing (development convenience): Carbon/HIToolbox keyboard layouts,
//! `sw_vers`, `sysctl` uptime/load, and `pmset` battery.

use std::ffi::{CStr, CString, c_char, c_void};

use super::*;
use cometdesk_clip::{Battery, Load};

// Text Input Sources (HIToolbox) and Core Foundation, just enough to list and select the
// enabled keyboard layouts.
type CFRef = *const c_void;
const UTF8: u32 = 0x0800_0100;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    static kTISPropertyInputSourceID: CFRef;
    fn TISCreateInputSourceList(properties: CFRef, include_all: u8) -> CFRef;
    fn TISGetInputSourceProperty(source: CFRef, key: CFRef) -> CFRef;
    fn TISSelectInputSource(source: CFRef) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    fn CFStringCreateWithCString(alloc: CFRef, s: *const c_char, enc: u32) -> CFRef;
    fn CFStringGetCString(s: CFRef, buf: *mut c_char, size: isize, enc: u32) -> u8;
    fn CFDictionaryCreate(
        alloc: CFRef,
        keys: *const CFRef,
        values: *const CFRef,
        n: isize,
        key_cb: *const c_void,
        value_cb: *const c_void,
    ) -> CFRef;
    fn CFArrayGetCount(a: CFRef) -> isize;
    fn CFArrayGetValueAtIndex(a: CFRef, i: isize) -> CFRef;
    fn CFRelease(r: CFRef);
}

/// The IDs of the enabled keyboard layouts (`com.apple.keylayout.German`).
fn layout_ids() -> Vec<String> {
    let mut ids = Vec::new();
    unsafe {
        let list = TISCreateInputSourceList(std::ptr::null(), 0);
        if list.is_null() {
            return ids;
        }
        for i in 0..CFArrayGetCount(list) {
            let id = TISGetInputSourceProperty(
                CFArrayGetValueAtIndex(list, i),
                kTISPropertyInputSourceID,
            );
            let mut buf = [0 as c_char; 256];
            if !id.is_null() && CFStringGetCString(id, buf.as_mut_ptr(), 256, UTF8) != 0 {
                let id = CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned();
                if id.starts_with("com.apple.keylayout.") {
                    ids.push(id);
                }
            }
        }
        CFRelease(list);
    }
    ids
}

pub fn set_layout(layout: &str) -> Result<(), String> {
    let id = layout.strip_prefix("mac:").ok_or("not a macOS layout")?;
    let c_id = CString::new(id).map_err(|_| "bad layout name")?;
    let status = unsafe {
        let value = CFStringCreateWithCString(std::ptr::null(), c_id.as_ptr(), UTF8);
        let key = kTISPropertyInputSourceID;
        let props = CFDictionaryCreate(
            std::ptr::null(),
            &key,
            &value,
            1,
            &raw const kCFTypeDictionaryKeyCallBacks,
            &raw const kCFTypeDictionaryValueCallBacks,
        );
        let list = TISCreateInputSourceList(props, 0);
        let status = if !list.is_null() && CFArrayGetCount(list) > 0 {
            Some(TISSelectInputSource(CFArrayGetValueAtIndex(list, 0)))
        } else {
            None
        };
        for r in [list, props, value] {
            if !r.is_null() {
                CFRelease(r);
            }
        }
        status
    };
    match status {
        Some(0) => Ok(()),
        Some(code) => Err(format!("macOS refused (status {code})")),
        None => Err("that layout is not enabled on the target".into()),
    }
}

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
    let mut layouts: Vec<String> = layout_ids()
        .into_iter()
        .map(|i| format!("mac:{i}"))
        .collect();
    if layouts.is_empty() {
        layouts = keyboard.iter().cloned().collect();
    }
    Info {
        layouts,
        keyboard,
        os_version: run("sw_vers", &["-productVersion"]).map(|v| format!("macOS {}", v.trim())),
        user: std::env::var("USER").ok(),
        uptime_secs: uptime(),
        battery: battery(),
        load: Some(load()),
        volume: crate::volume::read(),
        displays: crate::display::list(100),
        ..Info::default()
    }
}

/// `kern.boottime` is `{ sec = 1730000000, usec = 0 } Tue ...`.
fn uptime() -> Option<u64> {
    let out = run("sysctl", &["-n", "kern.boottime"])?;
    let sec: u64 = out
        .split("sec = ")
        .nth(1)?
        .split(',')
        .next()?
        .trim()
        .parse()
        .ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    now.checked_sub(sec)
}

/// `Now drawing from 'Battery Power'` then ` -InternalBattery-0 ... 85%; discharging; ...`.
fn battery() -> Option<Battery> {
    let out = run("pmset", &["-g", "batt"])?;
    let line = out.lines().find(|l| l.contains("InternalBattery"))?;
    let percent = line
        .split('%')
        .next()?
        .rsplit(|c: char| !c.is_ascii_digit())
        .next()?;
    Some(Battery {
        percent: percent.parse().ok()?,
        charging: !line.contains("discharging"),
    })
}

/// CPU: the processes' `%cpu` summed over the cores; memory: `vm_stat` against `hw.memsize`.
fn load() -> Load {
    let cores: f32 = run("sysctl", &["-n", "hw.ncpu"])
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(1.0);
    let cpu = run("ps", &["-A", "-o", "%cpu="]).map(|t| {
        let sum: f32 = t.lines().filter_map(|l| l.trim().parse::<f32>().ok()).sum();
        (sum / cores).clamp(0.0, 100.0) as u8
    });
    Load {
        cpu_percent: cpu,
        memory: memory(),
        disk: root_disk(),
    }
}

fn memory() -> Option<Usage> {
    let total: u64 = run("sysctl", &["-n", "hw.memsize"])?.trim().parse().ok()?;
    let vm = run("vm_stat", &[])?;
    let page: u64 = vm
        .lines()
        .next()?
        .split("page size of ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let pages = |name: &str| -> u64 {
        vm.lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| {
                l.split(':')
                    .nth(1)?
                    .trim()
                    .trim_end_matches('.')
                    .parse()
                    .ok()
            })
            .unwrap_or(0)
    };
    let free = (pages("Pages free") + pages("Pages inactive") + pages("Pages speculative")) * page;
    Some(Usage {
        used_mb: total.saturating_sub(free) / (1024 * 1024),
        total_mb: total / (1024 * 1024),
    })
}

pub fn focus() -> Option<Focus> {
    None
}

pub fn invalidate() {}
