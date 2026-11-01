//! Handshake events, target system info/restart alerts, user notifications, and clipboard sync.
//!
//! Called from `run.rs` on `Ready`, `Mismatch`, `Clip`, `Info`, `Notify`, and poll ticks.

use cometdesk_clip::{Content, Hello, Info};
use tokio::sync::watch;

use super::files::queue_clip_files;
use super::run::Run;
use super::settings::Settings;
use super::state::BridgeState;
use super::supervise::End;
use super::types::{Note, restarted};
use super::update::queue_update;

impl<S: Fn(BridgeState), W: Fn() + Send + Sync> Run<'_, S, W> {
    pub(super) fn on_ready(
        &mut self,
        peer: Hello,
        settings: &watch::Receiver<Settings>,
    ) -> Option<End> {
        tracing::info!(agent = %peer.name, os = %peer.os, "clipboard bridge connected");
        (self.set)(BridgeState::Connected(peer.clone(), Info::default()));
        if let Err(e) = self.ep.set_policy(settings.borrow().policy()) {
            return Some(End::Lost(e.to_string(), true));
        }
        queue_update(&peer, self.memory, &mut self.queue, self.notes);
        None
    }

    pub(super) fn on_mismatch(&mut self, peer: Hello) -> Option<End> {
        tracing::info!(agent = %peer.name, protocol = peer.protocol, "agent speaks another protocol");
        if peer.protocol > cometdesk_clip::PROTOCOL {
            return Some(End::Lost(
                "the agent on the target is newer than this app".into(),
                true,
            ));
        }
        if !queue_update(&peer, self.memory, &mut self.queue, self.notes) {
            return Some(End::AgentOutdated(peer.protocol));
        }
        (self.set)(BridgeState::Updating);
        None
    }

    pub(super) async fn on_clip(&mut self, content: Content, settings: &watch::Receiver<Settings>) {
        if settings.borrow().receives() {
            tracing::info!(?content, "clipboard from the target");
            self.local.apply(content).await;
        }
    }

    pub(super) fn on_info(&mut self, info: Info) {
        tracing::info!(?info, "target info");
        if let Some(up) = info.uptime_secs {
            if restarted(self.memory.uptime, up) {
                let _ = self.notes.send(Note::Target {
                    title: "Target restarted".into(),
                    body: "The target booted again and its agent is back.".into(),
                });
                (self.wake)();
            }
            self.memory.uptime = Some(up);
        }
        if let Some(peer) = self.ep.peer() {
            (self.set)(BridgeState::Connected(peer.clone(), info));
        }
    }

    pub(super) fn on_notify(&mut self, title: String, body: String) {
        let _ = self.notes.send(Note::Target { title, body });
        (self.wake)();
    }

    pub(super) async fn poll_clipboard(&mut self, up: bool, settings: &watch::Receiver<Settings>) {
        if let Some(content) = self.local.poll().await
            && up
            && settings.borrow().sends()
        {
            tracing::info!(?content, "clipboard to the target");
            if let Content::Files(paths) = content {
                queue_clip_files(
                    paths,
                    &mut self.queue,
                    &mut self.batches,
                    &mut self.next_batch,
                    self.notes,
                );
            } else if let Err(e) = self.ep.send_clip(&content) {
                tracing::warn!(%e, "cannot send clipboard item");
            }
        }
    }
}
