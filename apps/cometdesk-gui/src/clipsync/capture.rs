//! Target screenshot requests, PNG/failure replies, and capture timeout expiry.
//!
//! Called from `run.rs` when `ClipBridge::capture` requests arrive or screenshot events fire.

use std::time::Duration;

use tokio::time::Instant;

use super::run::Run;
use super::state::BridgeState;
use super::types::CaptureRequest;

/// A 4K screenshot is several MB through the relay.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(20);

impl<S: Fn(BridgeState), W: Fn() + Send + Sync> Run<'_, S, W> {
    pub(super) fn on_screenshot(&mut self, id: u32, png: Vec<u8>) {
        if let Some((tx, _)) = self.captured.remove(&id) {
            let _ = tx.send(Ok(png));
            (self.wake)();
        }
    }

    pub(super) fn on_capture_failed(&mut self, id: u32, reason: String) {
        if let Some((tx, _)) = self.captured.remove(&id) {
            let _ = tx.send(Err(reason));
            (self.wake)();
        }
    }

    pub(super) fn on_capture_request(&mut self, up: bool, req: CaptureRequest) {
        if !up {
            let _ = req.reply.send(Err("the agent is not connected".into()));
            return;
        }
        match self.ep.send_capture(req.display) {
            Ok(id) => {
                self.captured.insert(id, (req.reply, Instant::now()));
            }
            Err(e) => {
                let _ = req.reply.send(Err(e.to_string()));
            }
        }
    }

    pub(super) fn expire_captures(&mut self) {
        self.captured
            .retain(|_, (_, at)| at.elapsed() <= CAPTURE_TIMEOUT);
    }
}
