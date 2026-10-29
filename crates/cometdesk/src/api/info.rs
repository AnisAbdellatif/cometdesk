//! Device identity: `GET /api/info` and `GET /api/upgrade/version`.

use serde::Deserialize;

use crate::{Result, Session};

/// `GET /api/info?fields=system`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct SystemInfo {
    pub kvmd: Version,
    pub streamer: StreamerApp,
    pub platform: Platform,
    pub kernel: Kernel,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Version {
    pub version: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct StreamerApp {
    pub app: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Platform {
    pub base: String,
    pub board: String,
    pub model: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub video: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Kernel {
    pub machine: String,
    pub release: String,
    pub system: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct InfoResult {
    system: SystemInfo,
}

/// GL.iNet model and firmware (`GET /api/upgrade/version`), e.g. `RM1`, `V1.10.1 release3`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Firmware {
    pub model: String,
    pub version: String,
}

impl Session {
    pub async fn system_info(&self) -> Result<SystemInfo> {
        Ok(self
            .get::<InfoResult>("info", &[("fields", "system")])
            .await?
            .system)
    }

    pub async fn firmware(&self) -> Result<Firmware> {
        self.get("upgrade/version", &[]).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::fixtures;

    #[test]
    fn parses_info() {
        let info: InfoResult = fixtures::load("info");
        assert_eq!(info.system.kvmd.version, "4.82");
        assert_eq!(info.system.streamer.app, "ustreamer");
        assert_eq!(info.system.kernel.machine, "armv7l");
        assert!(info.system.platform.base.contains("RV1126"));
    }

    #[test]
    fn parses_firmware() {
        let fw: Firmware = fixtures::load("upgrade_version");
        assert_eq!(fw.model, "RM1");
        assert!(fw.version.starts_with("V1.10.1"));
    }
}
