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

pub fn staging_dir() -> PathBuf {
    cometdesk_clip::default_staging_dir()
}

pub fn save_secret(secret: &Secret) -> io::Result<PathBuf> {
    let path = secret_path()?;
    if let Some(parent) = path.parent() {
        cometdesk_clip::create_restricted_dir_all(parent)?;
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    use io::Write;
    use zeroize::Zeroize;
    let mut code = secret.to_code();
    let mut file = opts.open(&path)?;
    let res = file.write_all(code.as_bytes());
    code.zeroize();
    res?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(path)
}

pub fn load_secret() -> io::Result<Option<Secret>> {
    let path = secret_path()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&path) {
            let mut perms = meta.permissions();
            if perms.mode() & 0o077 != 0 {
                perms.set_mode(0o600);
                let _ = std::fs::set_permissions(&path, perms);
            }
        }
    }
    use zeroize::Zeroize;
    match std::fs::read_to_string(&path) {
        Ok(mut s) => {
            let secret = Secret::from_code(s.trim());
            s.zeroize();
            secret
                .map(Some)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "corrupted pairing file"))
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_round_trip_and_permissions() {
        let dir = std::env::temp_dir().join(format!("cometdesk-cfg-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let secret = Secret::generate();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let secret_file = dir.join("pairing");
            cometdesk_clip::create_restricted_dir_all(&dir).unwrap();
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            use std::io::Write;
            use zeroize::Zeroize;
            let mut code = secret.to_code();
            opts.open(&secret_file)
                .unwrap()
                .write_all(code.as_bytes())
                .unwrap();
            code.zeroize();
            let meta = std::fs::metadata(&secret_file).unwrap();
            assert_eq!(meta.permissions().mode() & 0o777, 0o600);
            let dir_meta = std::fs::metadata(&dir).unwrap();
            assert_eq!(dir_meta.permissions().mode() & 0o777, 0o700);

            let mut loaded = std::fs::read_to_string(&secret_file).unwrap();
            let loaded_secret = Secret::from_code(loaded.trim()).unwrap();
            loaded.zeroize();
            assert_eq!(loaded_secret, secret);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
