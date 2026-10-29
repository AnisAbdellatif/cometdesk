//! Replaces the running program with one the GUI sent, then starts it. The new program must
//! answer `--version` before it is put in place, so a download for the wrong platform or a
//! damaged file cannot leave the target without an agent.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Set for the successor, which waits for its predecessor to release the instance port.
pub const RESTARTED: &str = "COMETDESK_AGENT_RESTARTED";

/// Puts `staged` where the running program is; returns that path (read it before the swap:
/// Linux reports a replaced program as `... (deleted)`).
pub fn install(staged: &Path) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find the agent program: {e}"))?;
    let name = exe
        .file_name()
        .ok_or("the agent program has no name")?
        .to_string_lossy()
        .into_owned();
    let new = exe.with_file_name(format!("{name}.new"));
    std::fs::copy(staged, &new).map_err(|e| format!("cannot write {}: {e}", new.display()))?;
    let _ = std::fs::remove_file(staged);
    let swapped = prepare(&new).and_then(|()| swap(&exe, &new));
    if swapped.is_err() {
        let _ = std::fs::remove_file(&new);
    }
    swapped.map(|()| exe)
}

#[cfg(unix)]
fn prepare(new: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(new, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    check(new)
}

#[cfg(windows)]
fn prepare(new: &Path) -> Result<(), String> {
    check(new)
}

fn check(new: &Path) -> Result<(), String> {
    let out = Command::new(new)
        .arg("--version")
        .output()
        .map_err(|e| format!("the new agent does not run here: {e}"))?;
    if out.status.success() && String::from_utf8_lossy(&out.stdout).starts_with("cometdesk-agent ")
    {
        Ok(())
    } else {
        Err("the new agent did not identify itself".into())
    }
}

/// A running program cannot be overwritten on Windows, but it can be renamed.
#[cfg(windows)]
fn swap(exe: &Path, new: &Path) -> Result<(), String> {
    let old = exe.with_extension("old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).map_err(|e| format!("cannot move the running agent: {e}"))?;
    std::fs::rename(new, exe).map_err(|e| {
        let _ = std::fs::rename(&old, exe);
        format!("cannot install the new agent: {e}")
    })
}

#[cfg(unix)]
fn swap(exe: &Path, new: &Path) -> Result<(), String> {
    std::fs::rename(new, exe).map_err(|e| format!("cannot install the new agent: {e}"))
}

/// Removes what an update left behind; called when the agent starts.
pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old"));
    }
}

/// Starts `exe` with `args` and ends this process.
pub fn restart(exe: &Path, args: &[String]) -> ! {
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .env(RESTARTED, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
        cmd.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    let _ = cmd.spawn();
    std::process::exit(0)
}
