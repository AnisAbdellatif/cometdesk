//! Inbound frame deframing, Noise decryption, and `Msg` to `Event` dispatch on `Endpoint`.
//! Entry point: `Endpoint::receive`.

use sha2::{Digest, Sha256};

use crate::content::Content;
use crate::error::{Error, Result};
use crate::limits::{
    MAX_CLIP_FILES, MAX_FILE, MAX_ITEM, MAX_NOISE_MSG, MAX_NOTIFY_BODY, MAX_NOTIFY_TITLE,
    MAX_UPDATE, PROTOCOL, UPDATE_PROTOCOL,
};
use crate::protocol::{Purpose, Role};
use crate::wire::{Event, Incoming, Kind, Msg, Noise, cut};

use super::Endpoint;

impl Endpoint {
    /// Feeds received bytes; returns the resulting events. Any error ends the session.
    pub fn receive(&mut self, bytes: &[u8]) -> Result<Vec<Event>> {
        self.inbuf.extend_from_slice(bytes);
        let mut events = Vec::new();
        loop {
            if self.inbuf.len() < 4 {
                break;
            }
            let len = u32::from_be_bytes(self.inbuf[..4].try_into().unwrap()) as usize;
            if len == 0 || len > MAX_NOISE_MSG {
                return Err(Error::Frame("length out of range"));
            }
            if self.inbuf.len() < 4 + len {
                break;
            }
            let frame: Vec<u8> = self.inbuf.drain(..4 + len).skip(4).collect();
            self.on_frame(&frame, &mut events)?;
        }
        Ok(events)
    }

    fn on_frame(&mut self, frame: &[u8], events: &mut Vec<Event>) -> Result<()> {
        // The plaintext never exceeds the ciphertext. Own the buffer (not a `self` borrow) so
        // the arms can still call `&mut self` methods; restore it for the next message.
        let mut plain = std::mem::take(&mut self.scratch);
        plain.resize(frame.len(), 0);
        let (limited, peer_unknown) = (self.limited, self.peer.is_none());
        let result = match &mut self.noise {
            Noise::Handshake(hs) => hs
                .read_message(frame, &mut plain)
                .map_err(|_| Error::Handshake)
                .and_then(|_| {
                    if self.role == Role::Agent {
                        self.write_handshake()
                    } else {
                        self.maybe_finish_handshake()
                    }
                }),
            Noise::Transport(t) => t
                .read_message(frame, &mut plain)
                .map_err(|_| Error::Crypto)
                .and_then(|n| match postcard::from_bytes(&plain[..n]) {
                    Ok(msg) => Ok(Some(msg)),
                    // What another version sent may not decode; only updates matter then.
                    Err(_) if limited => Ok(None),
                    // The first message is the peer's hello: if it cannot be read, the peer
                    // predates the hello's current layout.
                    Err(_) if peer_unknown => Err(Error::Version(0)),
                    Err(e) => Err(Error::Message(e.to_string())),
                })
                .and_then(|msg| msg.map_or(Ok(()), |m| self.on_msg(m, events))),
            Noise::Failed => Err(Error::Handshake),
        };
        plain.clear();
        self.scratch = plain;
        result
    }

    fn on_msg(&mut self, msg: Msg, events: &mut Vec<Event>) -> Result<()> {
        if self.limited {
            let allowed = match &msg {
                Msg::Ping | Msg::FileData { .. } | Msg::FileEnd { .. } | Msg::FileCancel { .. } => {
                    true
                }
                Msg::FileOffer { purpose, .. } => *purpose == Purpose::Update,
                _ => false,
            };
            if !allowed {
                return Ok(());
            }
        }
        match msg {
            Msg::Hello(h) => {
                if h.protocol != PROTOCOL {
                    if h.protocol >= UPDATE_PROTOCOL {
                        self.limited = true;
                        self.peer = Some(h.clone());
                        events.push(Event::Mismatch(h));
                        return Ok(());
                    }
                    return Err(Error::Version(h.protocol));
                }
                self.peer = Some(h.clone());
                events.push(Event::Ready(h));
            }
            Msg::Info(i) => {
                if self.role == Role::Client {
                    events.push(Event::Info(i));
                }
            }
            Msg::Capture { id, display } if self.role == Role::Agent => {
                events.push(Event::Capture { id, display });
            }
            Msg::CaptureFailed { id, reason } if self.role == Role::Client => {
                events.push(Event::CaptureFailed { id, reason });
            }
            Msg::Capture { .. } | Msg::CaptureFailed { .. } => {
                return Err(Error::Message("capture message for the wrong role".into()));
            }
            Msg::Notify { title, body } if self.role == Role::Client => {
                events.push(Event::Notify {
                    title: cut(&title, MAX_NOTIFY_TITLE),
                    body: cut(&body, MAX_NOTIFY_BODY),
                });
            }
            Msg::Notify { .. } => {
                return Err(Error::Message("notification for the wrong role".into()));
            }
            Msg::FileOffer {
                id,
                name,
                size,
                purpose,
            } => {
                if size > MAX_FILE || (purpose == Purpose::Update && size > MAX_UPDATE) {
                    return Err(Error::Message("file too large".into()));
                }
                // Shared with outgoing transfers; a peer only ever needs one clipboard batch open.
                if self.files.len() >= 4 * MAX_CLIP_FILES {
                    return Err(Error::Message("too many files at once".into()));
                }
                self.files.insert(id, Sha256::new());
                events.push(Event::FileOffer {
                    id,
                    name,
                    size,
                    purpose,
                });
            }
            Msg::FilesReady { ids } => {
                if ids.len() > MAX_CLIP_FILES {
                    return Err(Error::Message("too many files".into()));
                }
                events.push(Event::FilesReady { ids });
            }
            Msg::FileData { id, data } => {
                // Unknown ids are a cancelled or refused file whose data was already in
                // flight: drop it quietly.
                if let Some(hash) = self.files.get_mut(&id) {
                    hash.update(&data);
                    events.push(Event::FileData { id, data });
                }
            }
            Msg::FileEnd { id, sha256 } => {
                if let Some(hash) = self.files.remove(&id) {
                    let intact = <[u8; 32]>::from(hash.finalize()) == sha256;
                    events.push(Event::FileEnd { id, intact });
                }
            }
            Msg::FileCancel { id } => {
                self.files.remove(&id);
                events.push(Event::FileCancel { id });
            }
            Msg::FileResult { id, result } => {
                events.push(Event::FileResult { id, result });
            }
            Msg::Power { id, action } if self.role == Role::Agent => {
                events.push(Event::Power { id, action });
            }
            Msg::SetLayout { id, layout } if self.role == Role::Agent => {
                events.push(Event::SetLayout { id, layout });
            }
            Msg::Ack { id, result } if self.role == Role::Client => {
                events.push(Event::Ack { id, result });
            }
            Msg::SetVolume { id, level, muted } if self.role == Role::Agent => {
                events.push(Event::SetVolume { id, level, muted });
            }
            Msg::SetDisplay {
                id,
                display,
                mode,
                primary,
            } if self.role == Role::Agent => {
                events.push(Event::SetDisplay {
                    id,
                    display,
                    mode,
                    primary,
                });
            }
            Msg::ListDir { id, target } if self.role == Role::Agent => {
                events.push(Event::ListDir { id, target });
            }
            Msg::Fetch { id, dir, name } if self.role == Role::Agent => {
                events.push(Event::Fetch { id, dir, name });
            }
            Msg::DirList { id, result } if self.role == Role::Client => {
                events.push(Event::DirList { id, result });
            }
            Msg::Power { .. }
            | Msg::SetLayout { .. }
            | Msg::Ack { .. }
            | Msg::SetVolume { .. }
            | Msg::SetDisplay { .. }
            | Msg::ListDir { .. }
            | Msg::Fetch { .. }
            | Msg::DirList { .. } => {
                return Err(Error::Message("request for the wrong role".into()));
            }
            Msg::Policy(p) => {
                if self.role == Role::Agent {
                    events.push(Event::Policy(p));
                }
            }
            Msg::Start {
                id,
                kind,
                len,
                sha256,
            } => {
                let len = len as usize;
                if len > MAX_ITEM {
                    return Err(Error::TooLarge(len));
                }
                self.incoming = Some(Incoming {
                    id,
                    kind,
                    len,
                    sha256,
                    data: Vec::with_capacity(len),
                });
                if len == 0 {
                    self.finish_item(events)?;
                }
            }
            Msg::Chunk { id, data } => {
                let Some(inc) = self.incoming.as_mut().filter(|i| i.id == id) else {
                    return Err(Error::Message(format!("chunk for unknown item {id}")));
                };
                if inc.data.len() + data.len() > inc.len {
                    return Err(Error::Message("item longer than announced".into()));
                }
                inc.data.extend_from_slice(&data);
                if inc.data.len() == inc.len {
                    self.finish_item(events)?;
                }
            }
            Msg::Ping => self.send(&Msg::Pong)?,
            Msg::Pong => {}
            Msg::Insert { id, text, paste } if self.role == Role::Agent => {
                events.push(Event::Insert { id, text, paste });
            }
            Msg::InsertReply { id, reply } if self.role == Role::Client => {
                events.push(Event::InsertReply { id, reply });
            }
            Msg::InsertPasted { id } if self.role == Role::Agent => {
                events.push(Event::InsertPasted { id });
            }
            Msg::Insert { .. } | Msg::InsertReply { .. } | Msg::InsertPasted { .. } => {
                return Err(Error::Message("insert message for the wrong role".into()));
            }
        }
        Ok(())
    }

    fn finish_item(&mut self, events: &mut Vec<Event>) -> Result<()> {
        let inc = self.incoming.take().expect("item in progress");
        if <[u8; 32]>::from(Sha256::digest(&inc.data)) != inc.sha256 {
            return Err(Error::Message("item checksum mismatch".into()));
        }
        if inc.kind == Kind::Screenshot {
            events.push(Event::Screenshot {
                id: inc.id,
                png: inc.data,
            });
            return Ok(());
        }
        let content = match inc.kind {
            Kind::Text => Content::Text(
                String::from_utf8(inc.data)
                    .map_err(|_| Error::Message("text is not UTF-8".into()))?,
            ),
            Kind::Png => Content::from_png(&inc.data)?,
            Kind::Screenshot => unreachable!(),
        };
        events.push(Event::Clip(content));
        Ok(())
    }
}
