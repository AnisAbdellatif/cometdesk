//! Streams one file through an [`Endpoint`], a piece per [`FileSender::pump`], so the caller's
//! loop keeps serving everything else in between.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::{Endpoint, FILE_CHUNK, MAX_CLIP_FILES, Purpose};

pub struct FileSender {
    pub id: u32,
    file: File,
    pub total: u64,
    pub sent: u64,
}

pub enum Pump {
    /// This many bytes were queued.
    Sent(usize),
    /// Everything was sent and the end marker queued; the peer's `FileResult` follows.
    Finished,
}

impl FileSender {
    /// Opens `path` and queues the offer. The size is taken now; a file that changes
    /// afterwards fails the transfer.
    pub fn start(
        ep: &mut Endpoint,
        path: &Path,
        name: &str,
        purpose: Purpose,
    ) -> Result<FileSender, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        let total = file.metadata().map_err(|e| e.to_string())?.len();
        let id = ep
            .send_file_offer(name, total, purpose)
            .map_err(|e| e.to_string())?;
        Ok(FileSender {
            id,
            file,
            total,
            sent: 0,
        })
    }

    /// Queues the next piece, or the end. On an error the peer is told to drop the file.
    pub fn pump(&mut self, ep: &mut Endpoint) -> Result<Pump, String> {
        let left = self.total - self.sent;
        let mut buf = vec![0u8; FILE_CHUNK.min(left as usize)];
        let res = self.file.read_exact(&mut buf).map_err(|e| match e.kind() {
            std::io::ErrorKind::UnexpectedEof => "the file changed while sending".to_string(),
            _ => e.to_string(),
        });
        let res = res.and_then(|()| {
            if buf.is_empty() {
                ep.send_file_end(self.id)
            } else {
                ep.send_file_data(self.id, &buf)
            }
            .map_err(|e| e.to_string())
        });
        if let Err(e) = res {
            let _ = ep.send_file_cancel(self.id);
            return Err(e);
        }
        self.sent += buf.len() as u64;
        Ok(if buf.is_empty() {
            Pump::Finished
        } else {
            Pump::Sent(buf.len())
        })
    }

    pub fn cancel(self, ep: &mut Endpoint) {
        let _ = ep.send_file_cancel(self.id);
    }
}

/// The files behind copied `paths` with the names to send them under: a file by its name, a
/// folder's files by `folder/sub/file`. Links are not followed. Stops one past
/// [`MAX_CLIP_FILES`], so the caller can tell it is too many.
pub fn expand(paths: &[PathBuf]) -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<(PathBuf, String)>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = read.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            if out.len() > MAX_CLIP_FILES {
                return;
            }
            let (Some(name), Ok(kind)) = (e.file_name().to_str().map(str::to_owned), e.file_type())
            else {
                continue;
            };
            let rel = format!("{prefix}/{name}");
            if kind.is_file() {
                out.push((e.path(), rel));
            } else if kind.is_dir() && depth < 14 {
                walk(&e.path(), &rel, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    for p in paths {
        let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        match std::fs::symlink_metadata(p) {
            Ok(m) if m.is_file() => out.push((p.clone(), name.to_owned())),
            Ok(m) if m.is_dir() => walk(p, name, 0, &mut out),
            _ => {}
        }
        if out.len() > MAX_CLIP_FILES {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_expand_to_relative_names() {
        let dir = std::env::temp_dir().join(format!("cometdesk-expand-{}", std::process::id()));
        let tree = dir.join("pics");
        std::fs::create_dir_all(tree.join("sub")).unwrap();
        std::fs::write(tree.join("a.txt"), "x").unwrap();
        std::fs::write(tree.join("sub/b.txt"), "x").unwrap();
        std::fs::write(dir.join("solo.txt"), "x").unwrap();
        let names: Vec<String> = expand(&[tree, dir.join("solo.txt")])
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        assert_eq!(names, ["pics/a.txt", "pics/sub/b.txt", "solo.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
