//! Hardware capability flags (`GET /api/system/capability`).
//!
//! `GET /api/system/clients` (viewer list) exists but answers 403 to normal sessions on
//! V1.10.1; it is reserved for the device's own GUI, so it is not exposed here.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{Result, Session};

#[derive(Deserialize, Default)]
#[serde(default)]
struct CapabilityResult {
    capability: BTreeMap<String, String>,
}

impl Session {
    /// Raw hardware flags (`cpu_model`, `mipi`, `usb_version`, ...). Values are strings.
    pub async fn hardware_capability(&self) -> Result<BTreeMap<String, String>> {
        Ok(self
            .get::<CapabilityResult>("system/capability", &[])
            .await?
            .capability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::fixtures;

    #[test]
    fn parses_capability() {
        let c: CapabilityResult = fixtures::load("system_capability");
        assert_eq!(c.capability["cpu_model"], "rv1126");
        assert_eq!(c.capability["usb_version"], "2");
    }
}
