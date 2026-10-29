//! Display identity the device presents to the target (EDID). **Device-wide and persisted**:
//! the target re-detects its monitor when it changes, which may rearrange its windows.

use serde::Deserialize;

use crate::client::Body;
use crate::{Result, Session};

/// A preset the device offers (`GET /api/upgrade/edid_list`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct EdidPreset {
    pub key: String,
    /// E.g. `2560x1440/CometDesk/60Hz`.
    pub label: String,
    /// Hex bytes without separators.
    pub content: String,
    /// Whether the display advertises audio.
    pub audio: bool,
    pub hz: u32,
    pub is_default: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct EdidResult {
    edid: String,
}

/// Hex digits only, lowercase: the device returns its EDID spaced and wrapped, the preset
/// list returns it compact.
pub fn normalize(edid: &str) -> String {
    edid.chars()
        .filter(char::is_ascii_hexdigit)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

impl Session {
    /// The EDID the device currently presents, normalized with [`normalize`].
    pub async fn edid(&self) -> Result<String> {
        Ok(normalize(
            &self.get::<EdidResult>("upgrade/get_edid", &[]).await?.edid,
        ))
    }

    /// The presets the firmware ships.
    pub async fn edid_presets(&self) -> Result<Vec<EdidPreset>> {
        // Unlike the rest of the API, this route answers a bare JSON array, not the envelope.
        let (status, bytes) = self
            .request(reqwest::Method::GET, "upgrade/edid_list", &[], Body::None)
            .await?;
        if status != 200 {
            return Err(crate::client::status_error(status, &bytes));
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Presents `edid` (hex) to the target. The device rejects malformed data.
    pub async fn set_edid(&self, edid: &str) -> Result<()> {
        let (status, bytes) = self
            .request(
                reqwest::Method::POST,
                "upgrade/edid",
                &[],
                Body::Field("edid", edid),
            )
            .await?;
        crate::client::parse_envelope::<serde_json::Value>(status, &bytes)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_layout() {
        assert_eq!(normalize("00 FF\n        ab"), "00ffab");
    }
}
