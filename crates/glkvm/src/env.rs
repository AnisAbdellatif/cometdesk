//! Minimal `.env` loader for credentials.
//!
//! Values are taken literally: no `$VAR` expansion and no escape processing, because
//! passwords routinely contain `$` and `\`. Surrounding single or double quotes are removed.
//! Variables already set in the environment win.

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
            load_file(&candidate).ok()?;
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

pub fn load_file(path: &Path) -> std::io::Result<()> {
    for (key, value) in parse(&std::fs::read_to_string(path)?) {
        if std::env::var_os(&key).is_none() {
            // SAFETY: called at startup before other threads read the environment.
            unsafe { std::env::set_var(key, value) };
        }
    }
    Ok(())
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

    #[test]
    fn invalid_lines_are_skipped() {
        assert!(parse("no equals\n=value\nBAD KEY=x\n").is_empty());
    }
}
