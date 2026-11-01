//! Connection state machine (`BridgeState`) and human-readable status labels for the UI.
//!
//! Read `supervise.rs` and `run.rs` for state transitions.

use cometdesk_clip::{Hello, Info};

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum BridgeState {
    Connecting,
    /// The relay is not installed on this device (`bridge/install.sh`).
    NotInstalled,
    /// Relay reached; no agent connected on the target.
    WaitingForAgent,
    /// The agent answered with a different pairing code.
    Mismatch,
    /// The agent speaks another protocol version: set up the target again.
    AgentOutdated(u16),
    /// The agent is older than this app and is being sent a new program.
    Updating,
    /// The agent's hello, and what it last told us about the target.
    Connected(Hello, Info),
    Error(String),
}

impl BridgeState {
    pub fn label(&self) -> String {
        match self {
            BridgeState::Connecting => "connecting…".into(),
            BridgeState::NotInstalled => "bridge not installed on this device".into(),
            BridgeState::WaitingForAgent => "waiting for the agent on the target".into(),
            BridgeState::Mismatch => "agent has a different pairing code".into(),
            BridgeState::AgentOutdated(_) => {
                "the agent on the target is outdated: set up the target again".into()
            }
            BridgeState::Updating => "updating the agent on the target…".into(),
            BridgeState::Connected(h, _) => format!("connected to {} ({})", h.name, h.os),
            BridgeState::Error(e) => format!("error: {e}"),
        }
    }
}
