//! The clipboard-bridge relay (`/glkvm-bridge/*`). Not GL firmware: our kvmd extra from
//! `bridge/` in this repo (docs/clipboard-bridge.md). It sits outside `/api`, behind the same
//! nginx login check.

use serde::Deserialize;

use crate::client::{Body, status_error};
use crate::{Result, Session};

/// WebSocket path of the relay's client side, for [`Session::ws_connect`].
pub const BRIDGE_WS: &str = "glkvm-bridge/ws";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct BridgeSide {
    pub connected: bool,
    /// Paired with the other side in the current session.
    pub paired: bool,
    pub peer: Option<String>,
    pub sessions: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct BridgeStatus {
    pub version: String,
    /// How the agent reaches the relay (`rndis`).
    pub link: String,
    pub agent: BridgeSide,
    pub client: BridgeSide,
}

impl Session {
    /// `GET /glkvm-bridge/status`. `None`: the bridge is not installed on this device.
    pub async fn bridge_status(&self) -> Result<Option<BridgeStatus>> {
        // The session's base URL is `https://<host>/api/`.
        let (status, body) = self
            .request(
                reqwest::Method::GET,
                "../glkvm-bridge/status",
                &[],
                Body::None,
            )
            .await?;
        match status {
            200 => Ok(Some(serde_json::from_slice(&body)?)),
            404 => Ok(None),
            s => Err(status_error(s, &body)),
        }
    }
}
