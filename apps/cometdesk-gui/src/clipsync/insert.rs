//! Text insertion requests, paste keystroke replies, and insert timeout expiry.
//!
//! Called from `run.rs` when `ClipBridge::insert` requests arrive or `Event::InsertReply` fires.

use std::time::Duration;

use cometdesk_clip::InsertReply;
use tokio::time::Instant;

use super::run::Run;
use super::state::BridgeState;
use super::supervise::End;
use super::types::{InsertRequest, paste_keys};

pub(super) const INSERT_TIMEOUT: Duration = Duration::from_secs(5);

impl<S: Fn(BridgeState), W: Fn() + Send + Sync> Run<'_, S, W> {
    pub(super) fn on_insert_reply(&mut self, id: u32, reply: InsertReply) -> Option<End> {
        let (tx, hid, _) = self.pending.remove(&id)?;
        let outcome = match reply {
            InsertReply::Inserted => Ok("Inserted on the target".into()),
            InsertReply::PasteNow(key) => {
                for cmd in crate::actions::combo(paste_keys(key)) {
                    hid.send(cmd);
                }
                if let Err(e) = self.ep.send_insert_pasted(id) {
                    return Some(End::Lost(e.to_string(), true));
                }
                Ok("Pasted on the target".into())
            }
            InsertReply::Failed(e) => Err(e),
        };
        let _ = tx.send(outcome);
        (self.wake)();
        None
    }

    pub(super) fn on_insert_request(&mut self, up: bool, req: InsertRequest) {
        if !up {
            let _ = req.reply.send(Err("the agent is not connected".into()));
            return;
        }
        match self.ep.send_insert(&req.text, req.paste) {
            Ok(id) => {
                self.pending
                    .insert(id, (req.reply, req.hid, Instant::now()));
            }
            Err(e) => {
                let _ = req.reply.send(Err(e.to_string()));
            }
        }
    }

    pub(super) fn expire_inserts(&mut self) {
        let expired: Vec<u32> = self
            .pending
            .iter()
            .filter(|(_, (_, _, at))| at.elapsed() > INSERT_TIMEOUT)
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            if let Some((tx, _, _)) = self.pending.remove(&id) {
                let _ = tx.send(Err("the agent did not answer".into()));
                (self.wake)();
            }
        }
    }
}
