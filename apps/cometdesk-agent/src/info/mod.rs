//! What the agent can tell the GUI about the target besides the clipboard: keyboard layouts,
//! session lock state, monitors, input method, focused application, account, uptime, battery,
//! OS version, and CPU, memory and disk use. Polled, because all of it changes while the agent
//! runs. Every field is best effort and `None` or empty where the OS offers no cheap way to ask.

#[cfg(all(unix, not(target_os = "macos")))]
#[path = "linux.rs"]
mod sys;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod sys;
#[cfg(windows)]
#[path = "windows.rs"]
mod sys;

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
