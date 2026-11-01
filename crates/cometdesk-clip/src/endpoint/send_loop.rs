//! Outbound message construction, item chunking, and Noise frame encryption on `Endpoint`.
//! Entry points: `Endpoint::send_*`, `Endpoint::set_policy`, and `Endpoint::ping`.

use sha2::{Digest, Sha256};

use crate::content::Content;
use crate::error::{Error, Result};
use crate::limits::{
    CHUNK, FILE_CHUNK, MAX_FILE, MAX_INSERT, MAX_ITEM, MAX_NOISE_MSG, MAX_NOTIFY_BODY,
    MAX_NOTIFY_TITLE, MAX_UPDATE, TAG_LEN,
};
use crate::protocol::{
    DirTarget, Info, InsertReply, Listing, PastePref, Policy, PowerAction, Purpose,
};
use crate::wire::{Kind, Msg, Noise, cut, push_frame};

use super::Endpoint;

impl Endpoint {
    pub fn set_policy(&mut self, policy: Policy) -> Result<()> {
        self.send(&Msg::Policy(policy))
    }

    /// Asks the agent to insert `text` at the target's focus. Returns the request id.
    pub fn send_insert(&mut self, text: &str, paste: PastePref) -> Result<u32> {
        if !self.is_ready() {
            return Err(Error::NotReady);
        }
        if text.len() > MAX_INSERT {
            return Err(Error::TooLarge(text.len()));
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.send(&Msg::Insert {
            id,
            text: text.to_owned(),
            paste,
        })?;
        Ok(id)
    }

    pub fn send_insert_reply(&mut self, id: u32, reply: InsertReply) -> Result<()> {
        self.send(&Msg::InsertReply { id, reply })
    }

    pub fn send_insert_pasted(&mut self, id: u32) -> Result<()> {
        self.send(&Msg::InsertPasted { id })
    }

    pub fn send_info(&mut self, info: &Info) -> Result<()> {
        self.send(&Msg::Info(info.clone()))
    }

    pub fn send_capture(&mut self, display: Option<u32>) -> Result<u32> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.send(&Msg::Capture { id, display })?;
        Ok(id)
    }

    pub fn send_power(&mut self, action: PowerAction) -> Result<u32> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.send(&Msg::Power { id, action })?;
        Ok(id)
    }

    pub fn send_set_layout(&mut self, layout: &str) -> Result<u32> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.send(&Msg::SetLayout {
            id,
            layout: layout.to_owned(),
        })?;
        Ok(id)
    }

    pub fn send_set_volume(&mut self, level: Option<u8>, muted: Option<bool>) -> Result<u32> {
        let id = self.take_id();
        self.send(&Msg::SetVolume {
            id,
            level: level.map(|l| l.min(100)),
            muted,
        })?;
        Ok(id)
    }

    pub fn send_set_display(
        &mut self,
        display: u32,
        mode: Option<(u32, u32)>,
        primary: bool,
    ) -> Result<u32> {
        let id = self.take_id();
        self.send(&Msg::SetDisplay {
            id,
            display,
            mode,
            primary,
        })?;
        Ok(id)
    }

    pub fn send_list_dir(&mut self, target: DirTarget) -> Result<u32> {
        let id = self.take_id();
        self.send(&Msg::ListDir { id, target })?;
        Ok(id)
    }

    pub fn send_dir_list(
        &mut self,
        id: u32,
        result: std::result::Result<Listing, String>,
    ) -> Result<()> {
        self.send(&Msg::DirList {
            id,
            result: result.map(Listing::fit),
        })
    }

    pub fn send_fetch(&mut self, dir: &str, name: &str) -> Result<u32> {
        let id = self.take_id();
        self.send(&Msg::Fetch {
            id,
            dir: dir.to_owned(),
            name: name.to_owned(),
        })?;
        Ok(id)
    }

    /// Starts sending a file: follow with [`Endpoint::send_file_data`] pieces and
    /// [`Endpoint::send_file_end`].
    pub fn send_file_offer(&mut self, name: &str, size: u64, purpose: Purpose) -> Result<u32> {
        if size > MAX_FILE || (purpose == Purpose::Update && size > MAX_UPDATE) {
            return Err(Error::Message("file too large".into()));
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.send(&Msg::FileOffer {
            id,
            name: name.to_owned(),
            size,
            purpose,
        })?;
        self.files.insert(id, Sha256::new());
        Ok(id)
    }

    pub fn send_file_data(&mut self, id: u32, data: &[u8]) -> Result<()> {
        if data.len() > FILE_CHUNK {
            return Err(Error::TooLarge(data.len()));
        }
        let hash = self
            .files
            .get_mut(&id)
            .ok_or_else(|| Error::Message(format!("no file {id} is being sent")))?;
        hash.update(data);
        self.send(&Msg::FileData {
            id,
            data: data.to_vec(),
        })
    }

    pub fn send_file_end(&mut self, id: u32) -> Result<()> {
        let hash = self
            .files
            .remove(&id)
            .ok_or_else(|| Error::Message(format!("no file {id} is being sent")))?;
        self.send(&Msg::FileEnd {
            id,
            sha256: hash.finalize().into(),
        })
    }

    /// Texts longer than [`MAX_NOTIFY_TITLE`] / [`MAX_NOTIFY_BODY`] characters are cut.
    pub fn send_notify(&mut self, title: &str, body: &str) -> Result<()> {
        self.send(&Msg::Notify {
            title: cut(title, MAX_NOTIFY_TITLE),
            body: cut(body, MAX_NOTIFY_BODY),
        })
    }

    pub fn send_files_ready(&mut self, ids: &[u32]) -> Result<()> {
        self.send(&Msg::FilesReady { ids: ids.to_vec() })
    }

    pub fn send_file_cancel(&mut self, id: u32) -> Result<()> {
        self.files.remove(&id);
        self.send(&Msg::FileCancel { id })
    }

    pub fn send_file_result(
        &mut self,
        id: u32,
        result: std::result::Result<String, String>,
    ) -> Result<()> {
        self.send(&Msg::FileResult { id, result })
    }

    pub fn send_ack(&mut self, id: u32, result: std::result::Result<(), String>) -> Result<()> {
        self.send(&Msg::Ack { id, result })
    }

    pub fn send_capture_failed(&mut self, id: u32, reason: String) -> Result<()> {
        self.send(&Msg::CaptureFailed { id, reason })
    }

    /// Answers a capture request with PNG bytes (see [`encode_png`]).
    pub fn send_screenshot(&mut self, id: u32, png: &[u8]) -> Result<()> {
        self.send_item(id, Kind::Screenshot, png)
    }

    pub fn ping(&mut self) -> Result<()> {
        self.send(&Msg::Ping)
    }

    /// Queues a clipboard item for the peer.
    pub fn send_clip(&mut self, content: &Content) -> Result<()> {
        if !self.is_ready() {
            return Err(Error::NotReady);
        }
        let (kind, wire) = match content {
            Content::Text(t) => (Kind::Text, t.as_bytes().to_vec()),
            Content::Image { .. } => (Kind::Png, content.to_png()?),
            Content::Files(_) => {
                return Err(Error::Message("files go through the file transfer".into()));
            }
        };
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.send_item(id, kind, &wire)
    }

    fn send_item(&mut self, id: u32, kind: Kind, wire: &[u8]) -> Result<()> {
        if wire.len() > MAX_ITEM {
            return Err(Error::TooLarge(wire.len()));
        }
        self.send(&Msg::Start {
            id,
            kind,
            len: wire.len() as u32,
            sha256: Sha256::digest(wire).into(),
        })?;
        for data in wire.chunks(CHUNK) {
            self.send(&Msg::Chunk {
                id,
                data: data.to_vec(),
            })?;
        }
        Ok(())
    }

    pub(crate) fn send(&mut self, msg: &Msg) -> Result<()> {
        let Noise::Transport(t) = &mut self.noise else {
            return Err(Error::NotReady);
        };
        let plain = postcard::to_allocvec(msg).map_err(|e| Error::Message(e.to_string()))?;
        debug_assert!(plain.len() + TAG_LEN <= MAX_NOISE_MSG);
        self.scratch.resize(plain.len() + TAG_LEN, 0);
        let n = t
            .write_message(&plain, &mut self.scratch)
            .map_err(|_| Error::Crypto)?;
        push_frame(&mut self.out, &self.scratch[..n]);
        self.scratch.clear();
        Ok(())
    }
}
