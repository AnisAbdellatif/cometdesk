//! Minimal `.env` loader for credentials.
//!
//! Values are taken literally: no `$VAR` expansion and no escape processing, because
//! passwords routinely contain `$` and `\`. Surrounding single or double quotes are removed.
//! Variables already set in the environment win.
//! On Unix a `.env` is used only if it is owned by the current user (or root) and not
//! writable by group or others: it can set the agents folder and the control socket, so one
//! planted in a shared parent directory must not be trusted.

use std::path::{Path, PathBuf};

/// Parses `KEY=VALUE` lines. Blank lines and `#` comments are skipped; `export ` is allowed.
pub fn parse(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return None;
            }
            let value = value.trim_end_matches(['\r', '\n']);
            let value = unquote(value.trim()).unwrap_or(value);
            Some((key.to_owned(), value.to_owned()))
        })
        .collect()
}

fn unquote(v: &str) -> Option<&str> {
    let b = v.as_bytes();
    (b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0])
        .then(|| &v[1..v.len() - 1])
}

/// Finds `.env` in the current directory or a parent and loads it into the process
/// environment. Returns the file used, if any.
pub fn load_dotenv() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join(".env");
        if candidate.is_file() {
            let text = read_trusted(&candidate)
                .map_err(|e| eprintln!("ignoring {}: {e}", candidate.display()))
                .ok()?;
            set_vars(&text);
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

pub fn load_file(path: &Path) -> std::io::Result<()> {
    set_vars(&read_trusted(path)?);
    Ok(())
}

fn set_vars(text: &str) {
    for (key, value) in parse(text) {
        if std::env::var_os(&key).is_none() {
            // SAFETY: called at startup before other threads read the environment.
            unsafe { std::env::set_var(key, value) };
        }
    }
}

/// Reads `path`, checking owner and mode on the opened file so it cannot be swapped between
/// the check and the read.
fn read_trusted(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: getuid has no preconditions and cannot fail.
        let me = unsafe { libc::getuid() };
        let meta = file.metadata()?;
        if !trusted(meta.uid(), meta.mode(), me) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "owned by another user or writable by group/others",
            ));
        }
    }
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text)
}

#[cfg(unix)]
fn trusted(owner: u32, mode: u32, me: u32) -> bool {
    (owner == me || owner == 0) && mode & 0o022 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get<'a>(v: &'a [(String, String)], k: &str) -> Option<&'a str> {
        v.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str())
    }

    #[test]
    fn values_are_literal() {
        let v = parse("PASS=a$HOME\\n#x\n");
        assert_eq!(get(&v, "PASS"), Some("a$HOME\\n#x"));
    }

    #[test]
    fn quotes_comments_export_and_blank_lines() {
        let v = parse("# comment\n\nexport HOST=10.0.0.1\nUSER = 'admin'\nP=\"x y\"\r\n");
        assert_eq!(get(&v, "HOST"), Some("10.0.0.1"));
        assert_eq!(get(&v, "USER"), Some("admin"));
        assert_eq!(get(&v, "P"), Some("x y"));
    }

    #[test]
    fn equals_inside_value_and_unbalanced_quote() {
        let v = parse("A=b=c\nB='open\n");
        assert_eq!(get(&v, "A"), Some("b=c"));
        assert_eq!(get(&v, "B"), Some("'open"));
    }

    #[cfg(unix)]
    #[test]
    fn only_own_unshared_files_are_trusted() {
        assert!(trusted(501, 0o100600, 501));
        assert!(trusted(0, 0o100644, 501));
        assert!(!trusted(502, 0o100600, 501));
        assert!(!trusted(501, 0o100664, 501));
        assert!(!trusted(501, 0o100646, 501));
    }

    #[test]
    fn invalid_lines_are_skipped() {
        assert!(parse("no equals\n=value\nBAD KEY=x\n").is_empty());
    }
}
