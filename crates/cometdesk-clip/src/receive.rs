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
/// Folders deeper than this are not copied.
const MAX_DEPTH: usize = 16;

const STAGING_KEEP: Duration = Duration::from_secs(24 * 60 * 60);

struct Receiving {
    file: File,
    part: PathBuf,
    /// The folder the finished file goes in.
    dir: PathBuf,
    name: String,
    /// What a clipboard batch gets for this file: the file, or the folder it is part of.
    top: PathBuf,
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
    /// Staging folders of the copied folders of this session, by their name.
    trees: HashMap<String, PathBuf>,
    staging: PathBuf,
    /// Keeps this session's staging folders apart from older ones.
    nonce: u128,
}

impl FileReceiver {
    /// `staging` holds clipboard files, each in its own sub-folder so names stay as sent.
    pub fn new(staging: PathBuf) -> FileReceiver {
        let _ = create_restricted_dir_all(&staging);
        remove_stale(&staging);
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        FileReceiver {
            open: HashMap::new(),
            staged: HashMap::new(),
            trees: HashMap::new(),
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
        let (dir, name, top) = match purpose {
            Purpose::Save => {
                let name = sanitize(name).ok_or("the file name is not usable")?;
                (
                    downloads().ok_or("cannot find the Downloads folder")?,
                    name,
                    None,
                )
            }
            Purpose::Update => {
                let dir = self.staging.join(format!("{:x}-{id}", self.nonce));
                (dir, "cometdesk-agent".to_owned(), None)
            }
            Purpose::Clipboard => {
                if size > MAX_CLIP_FILES_BYTES {
                    return Err("too large to copy through the clipboard".into());
                }
                let mut parts = sanitize_path(name).ok_or("the file name is not usable")?;
                let name = parts.pop().expect("at least one component");
                if parts.is_empty() {
                    let dir = self.staging.join(format!("{:x}-{id}", self.nonce));
                    let top = dir.join(&name);
                    (dir, name, Some(top))
                } else {
                    // A file of a copied folder: all of them share the folder's staging.
                    let next = self.trees.len();
                    let root = self
                        .trees
                        .entry(parts[0].clone())
                        .or_insert_with(|| self.staging.join(format!("{:x}-t{next}", self.nonce)))
                        .clone();
                    let top = root.join(&parts[0]);
                    let dir = parts.iter().fold(root, |d, p| d.join(p));
                    (dir, name, Some(top))
                }
            }
        };
        if purpose == Purpose::Save {
            fs::create_dir_all(&dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        } else {
            create_restricted_dir_all(&dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        let part = dir.join(format!(".cometdesk-{id}-{name}.part"));
        #[cfg(unix)]
        let file = {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&part)
                .map_err(|e| format!("cannot create the file: {e}"))?
        };
        #[cfg(not(unix))]
        let file = File::create(&part).map_err(|e| format!("cannot create the file: {e}"))?;
        self.open.insert(
            id,
            Receiving {
                file,
                part,
                top: top.unwrap_or_else(|| dir.join(&name)),
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
            Purpose::Clipboard | Purpose::Update => r.dir.join(&r.name),
        };
        fs::rename(&r.part, &dest).map_err(|e| format!("cannot save the file: {e}"))?;
        r.done = true;
        if r.purpose == Purpose::Clipboard {
            self.staged.insert(id, r.top.clone());
        }
        Ok(dest.display().to_string())
    }

    /// Whether `id` is an incoming agent program (see [`Purpose::Update`]).
    pub fn is_update(&self, id: u32) -> bool {
        self.open
            .get(&id)
            .is_some_and(|r| r.purpose == Purpose::Update)
    }

    pub fn cancel(&mut self, id: u32) {
        self.open.remove(&id);
    }

    /// The finished clipboard files for `ids`, in order; `None` if any is missing.
    pub fn take_staged(&mut self, ids: &[u32]) -> Option<Vec<PathBuf>> {
        let paths: Option<Vec<PathBuf>> = ids.iter().map(|i| self.staged.remove(i)).collect();
        let mut paths = paths.filter(|p| !p.is_empty())?;
        // Every file of a copied folder stages to the same top.
        let mut seen = std::collections::HashSet::new();
        paths.retain(|p| seen.insert(p.clone()));
        Some(paths)
    }
}

/// The default user-isolated staging directory for clipboard files and updates.
///
/// On Unix, prefers `$XDG_RUNTIME_DIR/cometdesk-clip` (mode 0700) or `~/.local/state/cometdesk-agent/staging` (mode 0700).
/// On Windows, uses `%LOCALAPPDATA%\cometdesk-agent\staging`.
pub fn default_staging_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) {
            return PathBuf::from(local).join("cometdesk-agent").join("staging");
        }
        if let Some(user) = std::env::var_os("USERPROFILE").filter(|v| !v.is_empty()) {
            return PathBuf::from(user)
                .join("AppData")
                .join("Local")
                .join("cometdesk-agent")
                .join("staging");
        }
        std::env::temp_dir().join("cometdesk-agent").join("staging")
    }
    #[cfg(not(windows))]
    {
        if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
            return PathBuf::from(runtime).join("cometdesk-clip");
        }
        if let Some(state) = std::env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty()) {
            return PathBuf::from(state).join("cometdesk-agent").join("staging");
        }
        if let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
            return PathBuf::from(home).join(".local/state/cometdesk-agent/staging");
        }
        std::env::temp_dir().join(format!("cometdesk-clip-{}", std::process::id()))
    }
}

/// Creates `path` and all ancestor directories, ensuring that newly created
/// directories (and `path` itself) have restricted permissions (0700 on Unix).
pub fn create_restricted_dir_all(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        use std::os::unix::fs::PermissionsExt;
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        builder.mode(0o700);
        builder.create(path)?;
        if let Ok(meta) = fs::metadata(path) {
            let mut perms = meta.permissions();
            if perms.mode() & 0o077 != 0 {
                perms.set_mode(0o700);
                let _ = fs::set_permissions(path, perms);
            }
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(path)
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

/// A relative path of at most [`MAX_DEPTH`] sanitized components, split at `/`.
fn sanitize_path(raw: &str) -> Option<Vec<String>> {
    let parts: Vec<String> = raw.split('/').map(sanitize).collect::<Option<Vec<_>>>()?;
    (parts.len() <= MAX_DEPTH).then_some(parts)
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
    fn relative_paths_keep_folders_but_not_escapes() {
        assert_eq!(
            sanitize_path("a/b/c.txt").unwrap(),
            ["a", "b", "c.txt"].map(String::from)
        );
        assert!(sanitize_path("a/../c.txt").is_none());
        assert!(sanitize_path("/etc/passwd").is_none());
        assert!(sanitize_path(&["x"; 20].join("/")).is_none());
    }

    #[test]
    fn copied_folder_stages_as_one_item() {
        let dir = std::env::temp_dir().join(format!("cometdesk-tree-{}", std::process::id()));
        let mut rx = FileReceiver::new(dir.clone());
        for (id, name) in [(1, "pics/a.txt"), (2, "pics/sub/b.txt"), (3, "solo.txt")] {
            rx.offer(id, name, 1, Purpose::Clipboard).unwrap();
            rx.data(id, b"x").unwrap();
            rx.end(id, true).unwrap();
        }
        let paths = rx.take_staged(&[1, 2, 3]).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths[0].ends_with("pics") && paths[0].join("sub/b.txt").is_file());
        assert!(paths[1].ends_with("solo.txt") && paths[1].is_file());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn existing_files_are_not_overwritten() {
        let dir = std::env::temp_dir().join(format!("cometdesk-files-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), "x").unwrap();
        fs::write(dir.join("a (1).txt"), "x").unwrap();
        assert_eq!(unique(&dir, "a.txt"), dir.join("a (2).txt"));
        assert_eq!(unique(&dir, "b"), dir.join("b"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn staging_dir_and_subdirs_have_restricted_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("cometdesk-perm-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut rx = FileReceiver::new(dir.clone());
        let meta = fs::metadata(&dir).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o700);

        rx.offer(1, "test.txt", 4, Purpose::Clipboard).unwrap();
        rx.data(1, b"test").unwrap();
        let path = rx.end(1, true).unwrap();
        let p = Path::new(&path);
        let parent = p.parent().unwrap();
        let parent_meta = fs::metadata(parent).unwrap();
        assert_eq!(parent_meta.permissions().mode() & 0o777, 0o700);
        let file_meta = fs::metadata(p).unwrap();
        assert_eq!(file_meta.permissions().mode() & 0o777, 0o600);

        let _ = fs::remove_dir_all(&dir);
    }
}
