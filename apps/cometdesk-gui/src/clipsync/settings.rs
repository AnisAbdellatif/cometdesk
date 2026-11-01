//! Clipboard direction and pause settings, plus policy conversion for the agent.
//!
//! Read `bridge.rs` for live updates via `ClipBridge::set_settings`.

use cometdesk_clip::Policy;

use crate::store::ClipDirection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub direction: ClipDirection,
    pub paused: bool,
}

impl Settings {
    pub(super) fn sends(self) -> bool {
        !self.paused && self.direction.sends()
    }
    pub(super) fn receives(self) -> bool {
        !self.paused && self.direction.receives()
    }
    pub(super) fn policy(self) -> Policy {
        Policy {
            agent_may_send: self.receives(),
        }
    }
}
