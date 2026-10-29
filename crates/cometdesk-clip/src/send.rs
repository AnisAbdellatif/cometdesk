//! Streams one file through an [`Endpoint`], a piece per [`FileSender::pump`], so the caller's
//! loop keeps serving everything else in between.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::{Endpoint, FILE_CHUNK, Purpose};

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
