//! Reconnection loop (`supervise`) and session termination reason (`End`).
//!
//! Connects to `/cometdesk-bridge/ws`, runs one session, and backs off while failing requests fast.

use std::time::Duration;

use cometdesk::Session;
use cometdesk::api::bridge::BRIDGE_WS;
use cometdesk_clip::Secret;
use tokio::sync::{mpsc, watch};

use super::LocalClipboard;
use super::run::run;
use super::settings::Settings;
use super::state::BridgeState;
use super::types::{
    AckRequest, CaptureRequest, DirRequest, FileRequest, InsertRequest, Memory, Note,
};

const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(15);
const RETRY_SLOW: Duration = Duration::from_secs(30);
/// While waiting to reconnect, how often to look for an agent already waiting on the relay.
const AGENT_CHECK: Duration = Duration::from_secs(2);

pub(super) enum End {
    Mismatch,
    NotInstalled,
    AgentOutdated(u16),
    /// Reason, and whether a session had been established.
    Lost(String, bool),
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn supervise(
    session: Session,
    secret: Secret,
    mut settings: watch::Receiver<Settings>,
    mut inserts: mpsc::UnboundedReceiver<InsertRequest>,
    mut captures: mpsc::UnboundedReceiver<CaptureRequest>,
    mut acks: mpsc::UnboundedReceiver<AckRequest>,
    mut dirs: mpsc::UnboundedReceiver<DirRequest>,
    mut files: mpsc::UnboundedReceiver<FileRequest>,
    notes: mpsc::UnboundedSender<Note>,
    state: watch::Sender<BridgeState>,
    wake: impl Fn() + Send + Sync + 'static,
) {
    let set = |s: BridgeState| {
        if state.send_if_modified(|cur| {
            let changed = *cur != s;
            *cur = s;
            changed
        }) {
            wake();
        }
    };
    let mut local = LocalClipboard::new().await;
    let mut memory = Memory::default();
    let mut backoff = BACKOFF_MIN;
    loop {
        set(BridgeState::Connecting);
        let end = match session.bridge_status().await {
            Ok(None) => End::NotInstalled,
            Err(e) => End::Lost(e.to_string(), false),
            Ok(Some(_)) => match session.ws_connect(BRIDGE_WS).await {
                Ok(ws) => {
                    run(
                        ws,
                        &session,
                        &secret,
                        &mut settings,
                        &mut inserts,
                        &mut captures,
                        &mut acks,
                        &mut dirs,
                        &mut files,
                        &notes,
                        &mut local,
                        &mut memory,
                        &set,
                        &wake,
                    )
                    .await
                }
                Err(e) => End::Lost(e.to_string(), false),
            },
        };
        let wait = match end {
            End::NotInstalled => {
                set(BridgeState::NotInstalled);
                RETRY_SLOW
            }
            End::Mismatch => {
                set(BridgeState::Mismatch);
                RETRY_SLOW
            }
            End::AgentOutdated(v) => {
                set(BridgeState::AgentOutdated(v));
                RETRY_SLOW
            }
            End::Lost(reason, was_up) => {
                tracing::info!(%reason, "clipboard bridge connection lost");
                set(BridgeState::Error(reason));
                if was_up {
                    backoff = BACKOFF_MIN;
                }
                let w = backoff;
                backoff = (backoff * 2).min(BACKOFF_MAX);
                w
            }
        };
        // Inserts asked for while disconnected fail now rather than after the wait.
        let deadline = tokio::time::Instant::now() + wait;
        // A freshly installed agent is waiting on the relay: don't make it wait out a long
        // backoff (up to RETRY_SLOW) that started before it existed.
        let mut agent_check = tokio::time::interval(AGENT_CHECK);
        agent_check.tick().await;
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => break,
                _ = agent_check.tick() => {
                    if let Ok(Some(st)) = session.bridge_status().await
                        && st.agent.connected
                        && !st.client.connected
                    {
                        break;
                    }
                }
                Some(req) = inserts.recv() => {
                    let _ = req.reply.send(Err("the agent is not connected".into()));
                }
                Some(req) = captures.recv() => {
                    let _ = req.reply.send(Err("the agent is not connected".into()));
                }
                Some(req) = acks.recv() => {
                    let _ = req.reply.send(Err("the agent is not connected".into()));
                }
                Some(req) = dirs.recv() => {
                    let _ = req.reply.send(Err("the agent is not connected".into()));
                }
                Some(req) = files.recv() => {
                    let _ = req.reply.send(Err("the agent is not connected".into()));
                }
            }
        }
    }
}
