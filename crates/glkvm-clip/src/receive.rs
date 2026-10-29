//! Saves files the peer sends: into the user's Downloads folder, or, for clipboard files, into
//! a staging folder. The sender picks only the name, which is sanitized here; data goes to a
//! `.part` file first and is renamed once it has arrived intact, so an interrupted transfer
//! never leaves something that looks finished.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::{MAX_CLIP_FILES_BYTES, Purpose};

/// Staged clipboard files older than this are removed the next time a receiver is created.
const STAGING_KEEP: Duration = Duration::from_secs(24 * 60 * 60);

struct Receiving {
    file: File,
    part: PathBuf,
    dir: PathBuf,
    name: String,
    size: u64,
    written: u64,
    purpose: Purpose,
    /// Set once renamed, so dropping does not delete the result.
    done: bool,
}

impl Drop for Receiving {
    fn drop(&mut self) {
        if !self.done {
            let _ = fs::remove_file(&self.part);
        }
    }
}

/// The files being received in one session; dropping it discards the unfinished ones.
pub struct FileReceiver {
    open: HashMap<u32, Receiving>,
    /// Finished clipboard files by transfer id, until [`FileReceiver::take_staged`].
    staged: HashMap<u32, PathBuf>,
    staging: PathBuf,
    /// Keeps this session's staging folders apart from older ones.
    nonce: u128,
}

impl FileReceiver {
    /// `staging` holds clipboard files, each in its own sub-folder so names stay as sent.
    pub fn new(staging: PathBuf) -> FileReceiver {
        remove_stale(&staging);
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        FileReceiver {
            open: HashMap::new(),
            staged: HashMap::new(),
            staging,
            nonce,
        }
    }

    pub fn offer(
        &mut self,
        id: u32,
        name: &str,
        size: u64,
        purpose: Purpose,
    ) -> Result<(), String> {
        let name = sanitize(name).ok_or("the file name is not usable")?;
        let dir = match purpose {
            Purpose::Save => downloads().ok_or("cannot find the Downloads folder")?,
            Purpose::Clipboard => {
                if size > MAX_CLIP_FILES_BYTES {
                    return Err("too large to copy through the clipboard".into());
                }
                self.staging.join(format!("{:x}-{id}", self.nonce))
            }
        };
        fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let part = dir.join(format!(".glkvm-{id}-{name}.part"));
        let file = File::create(&part).map_err(|e| format!("cannot create the file: {e}"))?;
        self.open.insert(
            id,
            Receiving {
                file,
                part,
                dir,
                name,
                size,
                written: 0,
                purpose,
                done: false,
            },
        );
        Ok(())
    }

    /// An error discards the file; the caller reports it.
    pub fn data(&mut self, id: u32, data: &[u8]) -> Result<(), String> {
        let Some(r) = self.open.get_mut(&id) else {
            return Ok(());
        };
        let res = if r.written + data.len() as u64 > r.size {
            Err("more data than announced".to_string())
        } else {
            r.file
                .write_all(data)
                .map_err(|e| format!("cannot write the file: {e}"))
        };
        match res {
            Ok(()) => {
                r.written += data.len() as u64;
                Ok(())
            }
            Err(e) => {
                self.open.remove(&id);
                Err(e)
            }
        }
    }

    /// The saved path.
    pub fn end(&mut self, id: u32, intact: bool) -> Result<String, String> {
        let mut r = self.open.remove(&id).ok_or("unknown file")?;
        if !intact || r.written != r.size {
            return Err("the file arrived damaged or incomplete".into());
        }
        r.file
            .sync_all()
            .map_err(|e| format!("cannot write the file: {e}"))?;
        // Clipboard folders are ours alone; Downloads may already hold the name.
        let dest = match r.purpose {
            Purpose::Save => unique(&r.dir, &r.name),
            Purpose::Clipboard => r.dir.join(&r.name),
        };
        fs::rename(&r.part, &dest).map_err(|e| format!("cannot save the file: {e}"))?;
        r.done = true;
        if r.purpose == Purpose::Clipboard {
            self.staged.insert(id, dest.clone());
        }
        Ok(dest.display().to_string())
    }

    pub fn cancel(&mut self, id: u32) {
        self.open.remove(&id);
    }

    /// The finished clipboard files for `ids`, in order; `None` if any is missing.
    pub fn take_staged(&mut self, ids: &[u32]) -> Option<Vec<PathBuf>> {
        let paths: Option<Vec<PathBuf>> = ids.iter().map(|i| self.staged.remove(i)).collect();
        paths.filter(|p| !p.is_empty())
    }
}

fn remove_stale(staging: &Path) {
    let Ok(entries) = fs::read_dir(staging) else {
        return;
    };
    for e in entries.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > STAGING_KEEP);
        if old {
            let _ = fs::remove_dir_all(e.path());
        }
    }
}

fn downloads() -> Option<PathBuf> {
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME");
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Ok(out) = std::process::Command::new("xdg-user-dir")
        .arg("DOWNLOAD")
        .output()
    {
        let dir = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if out.status.success()
            && !dir.is_empty()
            && Some(dir.as_str()) != home.as_deref().and_then(|h| h.to_str())
        {
            return Some(PathBuf::from(dir));
        }
    }
    Some(PathBuf::from(home?).join("Downloads"))
}

/// A bare, portable file name, or `None` if nothing usable is left. Applied on every OS, so a
/// file saved on Linux can be moved to Windows.
fn sanitize(raw: &str) -> Option<String> {
    let base = raw.rsplit(['/', '\\']).next()?;
    let mut name: String = base
        .chars()
        .map(|c| match c {
            c if c.is_control() => '_',
            '<' | '>' | ':' | '"' | '|' | '?' | '*' => '_',
            c => c,
        })
        .collect();
    name = name.trim_matches([' ', '.']).to_owned();
    while name.len() > 200 {
        name.pop();
    }
    if name.is_empty() {
        return None;
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|p| {
            stem.strip_prefix(p)
                .is_some_and(|n| n.len() == 1 && n != "0")
        });
    Some(if reserved { format!("_{name}") } else { name })
}

/// `name`, or `name (1).ext`, `name (2).ext`, … if it exists.
fn unique(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_lose_paths_and_bad_characters() {
        assert_eq!(sanitize("../../etc/passwd").as_deref(), Some("passwd"));
        assert_eq!(sanitize("C:\\x\\a:b.txt").as_deref(), Some("a_b.txt"));
        assert_eq!(sanitize("..").as_deref(), None);
        assert_eq!(sanitize("dir/").as_deref(), None);
        assert_eq!(sanitize("nul.txt").as_deref(), Some("_nul.txt"));
        assert_eq!(sanitize("com1").as_deref(), Some("_com1"));
        assert_eq!(sanitize("com0").as_deref(), Some("com0"));
        assert_eq!(sanitize("a\nb").as_deref(), Some("a_b"));
    }

    #[test]
    fn existing_files_are_not_overwritten() {
        let dir = std::env::temp_dir().join(format!("glkvm-files-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), "x").unwrap();
        fs::write(dir.join("a (1).txt"), "x").unwrap();
        assert_eq!(unique(&dir, "a.txt"), dir.join("a (2).txt"));
        assert_eq!(unique(&dir, "b"), dir.join("b"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
