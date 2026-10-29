//! Lets the GUI look through the target's file system and pick files to fetch. The GUI is the
//! paired user's own machine, so this shows what that user can read; nothing is written.

use std::path::{Path, PathBuf};

use cometdesk_clip::{DirEntry, DirTarget, Listing};

fn home() -> Option<PathBuf> {
    #[cfg(windows)]
    let var = "USERPROFILE";
    #[cfg(not(windows))]
    let var = "HOME";
    std::env::var_os(var).map(PathBuf::from)
}

/// A single path component the user can have clicked: no separators, not `..`.
fn entry_name(name: &str) -> Result<&str, String> {
    if name.is_empty() || name == ".." || name == "." || name.contains(['/', '\\', '\0']) {
        Err("not a file name".into())
    } else {
        Ok(name)
    }
}

pub fn list(target: DirTarget) -> Result<Listing, String> {
    let dir = match target {
        DirTarget::Home => home().ok_or("cannot find the home folder")?,
        DirTarget::Roots => return roots(),
        DirTarget::Open(p) => PathBuf::from(p),
        DirTarget::Child { dir, name } => Path::new(&dir).join(entry_name(&name)?),
        DirTarget::Parent(p) => match Path::new(&p).parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => return roots(),
        },
    };
    read(&dir)
}

/// The file `name` inside `dir`, if it is a regular file.
pub fn file(dir: &str, name: &str) -> Result<PathBuf, String> {
    let path = Path::new(dir).join(entry_name(name)?);
    match std::fs::metadata(&path) {
        Ok(m) if m.is_file() => Ok(path),
        Ok(_) => Err("not a file".into()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(windows)]
fn roots() -> Result<Listing, String> {
    let entries = ('A'..='Z')
        .map(|l| format!("{l}:\\"))
        .filter(|d| Path::new(d).exists())
        .map(|name| DirEntry {
            name,
            dir: true,
            size: 0,
        })
        .collect();
    Ok(Listing {
        path: String::new(),
        entries,
        truncated: false,
    })
}

#[cfg(not(windows))]
fn roots() -> Result<Listing, String> {
    read(Path::new("/"))
}

fn read(dir: &Path) -> Result<Listing, String> {
    let mut entries: Vec<DirEntry> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            // Follow links, so a link to a folder opens like one.
            let meta = std::fs::metadata(e.path()).or_else(|_| e.metadata()).ok()?;
            Some(DirEntry {
                name,
                dir: meta.is_dir(),
                size: if meta.is_dir() { 0 } else { meta.len() },
            })
        })
        .collect();
    entries.sort_by_cached_key(|e| (!e.dir, e.name.to_lowercase()));
    Ok(Listing {
        path: dir.display().to_string(),
        entries,
        truncated: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_folders_first_and_refuses_escapes() {
        let dir = std::env::temp_dir().join(format!("cometdesk-browse-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("zdir")).unwrap();
        std::fs::write(dir.join("b.txt"), "xy").unwrap();
        std::fs::write(dir.join("A.txt"), "x").unwrap();
        let l = list(DirTarget::Open(dir.display().to_string())).unwrap();
        let names: Vec<_> = l.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["zdir", "A.txt", "b.txt"]);
        assert_eq!(l.entries[2].size, 2);
        let child = list(DirTarget::Child {
            dir: l.path.clone(),
            name: "zdir".into(),
        })
        .unwrap();
        assert!(child.entries.is_empty() && child.path.ends_with("zdir"));
        assert!(
            list(DirTarget::Child {
                dir: l.path.clone(),
                name: "../x".into()
            })
            .is_err()
        );
        assert!(file(&l.path, "b.txt").is_ok());
        assert!(file(&l.path, "zdir").is_err());
        assert!(file(&l.path, "..").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
