//! Where the pairing secret lives, and the host name sent in `Hello`.

use std::io;
use std::path::PathBuf;

use cometdesk_clip::Secret;

fn dir() -> io::Result<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    let base =
        base.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no config directory"))?;
    let dir = base.join("cometdesk-agent");
    // Directory of the former name (`glkvm-agent`): keep the pairing.
    let old = base.join("glkvm-agent");
    if !dir.exists() && old.is_dir() {
        let _ = std::fs::rename(&old, &dir);
    }
    Ok(dir)
}

fn secret_path() -> io::Result<PathBuf> {
    Ok(dir()?.join("pairing"))
}

pub fn save_secret(secret: &Secret) -> io::Result<PathBuf> {
    let path = secret_path()?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    use io::Write;
    opts.open(&path)?.write_all(secret.to_code().as_bytes())?;
    Ok(path)
}

pub fn load_secret() -> io::Result<Option<Secret>> {
    match std::fs::read_to_string(secret_path()?) {
        Ok(s) => Secret::from_code(s.trim())
            .map(Some)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "corrupted pairing file")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn remove_secret() -> io::Result<()> {
    match std::fs::remove_file(secret_path()?) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Removes the autostart entry the installer created (bridge/target/install.*).
pub fn remove_autostart() -> io::Result<()> {
    if cfg!(windows) {
        // A non-zero exit also means "no such value": nothing to remove.
        std::process::Command::new("reg")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "cometdesk-agent",
                "/f",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;
        return Ok(());
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    if let Some(base) = base {
        match std::fs::remove_file(base.join("autostart/cometdesk-agent.desktop")) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

pub fn host_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "target".into())
}
