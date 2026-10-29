//! Lock, sleep, log off, restart and shut down the target, through the OS's own tools so the
//! session is closed cleanly. `Ok` means the OS accepted the request, not that it finished.

use std::process::Command;

use glkvm_clip::PowerAction;

pub fn run(action: PowerAction) -> Result<(), String> {
    let (program, args) = command(action);
    let mut cmd = Command::new(program);
    cmd.args(args);
    #[cfg(all(unix, not(target_os = "macos")))]
    if action == PowerAction::Logoff {
        // The agent runs inside the session it ends.
        let id = std::env::var("XDG_SESSION_ID").map_err(|_| "no XDG_SESSION_ID".to_string())?;
        cmd.arg(id);
    }
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000); // no console
    let out = cmd
        .output()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let why = String::from_utf8_lossy(&out.stderr);
    Err(format!("{program} failed ({}): {}", out.status, why.trim()))
}

#[cfg(windows)]
fn command(action: PowerAction) -> (&'static str, &'static [&'static str]) {
    match action {
        PowerAction::Lock => ("rundll32.exe", &["user32.dll,LockWorkStation"]),
        PowerAction::Sleep => ("rundll32.exe", &["powrprof.dll,SetSuspendState", "0,1,0"]),
        PowerAction::Logoff => ("shutdown.exe", &["/l"]),
        PowerAction::Restart => ("shutdown.exe", &["/r", "/t", "0"]),
        PowerAction::Shutdown => ("shutdown.exe", &["/s", "/t", "0"]),
    }
}

#[cfg(target_os = "macos")]
fn command(action: PowerAction) -> (&'static str, &'static [&'static str]) {
    match action {
        // Locks when "require password after sleep or screen saver" is on, which it is by default.
        PowerAction::Lock => ("pmset", &["displaysleepnow"]),
        PowerAction::Sleep => ("pmset", &["sleepnow"]),
        PowerAction::Logoff => (
            "osascript",
            &["-e", "tell application \"System Events\" to log out"],
        ),
        PowerAction::Restart => (
            "osascript",
            &["-e", "tell application \"System Events\" to restart"],
        ),
        PowerAction::Shutdown => (
            "osascript",
            &["-e", "tell application \"System Events\" to shut down"],
        ),
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn command(action: PowerAction) -> (&'static str, &'static [&'static str]) {
    match action {
        PowerAction::Lock => ("loginctl", &["lock-session"]),
        PowerAction::Sleep => ("systemctl", &["suspend"]),
        PowerAction::Logoff => ("loginctl", &["terminate-session"]),
        PowerAction::Restart => ("systemctl", &["reboot"]),
        PowerAction::Shutdown => ("systemctl", &["poweroff"]),
    }
}
