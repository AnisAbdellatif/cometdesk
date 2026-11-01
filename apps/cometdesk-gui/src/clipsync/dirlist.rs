//! Target directory listing and control/fetch ack requests, replies, and timeout expiry.
//!
//! Handles `ClipBridge::list_dir` and `ClipBridge::request` (power, layout, volume, display, fetch).

use tokio::time::Instant;

use super::insert::INSERT_TIMEOUT;
use super::run::Run;
use super::state::BridgeState;
use super::types::{AckRequest, AckResult, DirRequest, ListResult, Request};

impl<S: Fn(BridgeState), W: Fn() + Send + Sync> Run<'_, S, W> {
    pub(super) fn on_ack(&mut self, id: u32, result: AckResult) {
        if self.fetch_acks.remove(&id) && result.is_err() {
            self.fetches = self.fetches.saturating_sub(1);
        }
        if let Some((tx, _)) = self.acked.remove(&id) {
            let _ = tx.send(result);
            (self.wake)();
        }
    }

    pub(super) fn on_dir_list(&mut self, id: u32, result: ListResult) {
        if let Some((tx, _)) = self.listed.remove(&id) {
            let _ = tx.send(result);
            (self.wake)();
        }
    }

    pub(super) fn on_dir_request(&mut self, up: bool, req: DirRequest) {
        if !up {
            let _ = req.reply.send(Err("the agent is not connected".into()));
            return;
        }
        match self.ep.send_list_dir(req.target) {
            Ok(id) => {
                self.listed.insert(id, (req.reply, Instant::now()));
            }
            Err(e) => {
                let _ = req.reply.send(Err(e.to_string()));
            }
        }
    }

    pub(super) fn on_ack_request(&mut self, up: bool, req: AckRequest) {
        if !up {
            let _ = req.reply.send(Err("the agent is not connected".into()));
            return;
        }
        let sent = match &req.request {
            Request::Power(a) => self.ep.send_power(*a),
            Request::Layout(l) => self.ep.send_set_layout(l),
            Request::Volume { level, muted } => self.ep.send_set_volume(*level, *muted),
            Request::Display {
                index,
                mode,
                primary,
            } => self.ep.send_set_display(*index, *mode, *primary),
            Request::Fetch { dir, name } => {
                self.fetches += 1;
                self.ep.send_fetch(dir, name)
            }
        };
        match sent {
            Ok(id) => {
                if matches!(req.request, Request::Fetch { .. }) {
                    self.fetch_acks.insert(id);
                }
                self.acked.insert(id, (req.reply, Instant::now()));
            }
            Err(e) => {
                let _ = req.reply.send(Err(e.to_string()));
            }
        }
    }

    pub(super) fn expire_acks_and_dirs(&mut self) {
        self.acked
            .retain(|_, (_, at)| at.elapsed() <= INSERT_TIMEOUT);
        self.listed
            .retain(|_, (_, at)| at.elapsed() <= INSERT_TIMEOUT);
    }
}
