//! Wake-on-LAN (`/api/wol`): the device sends magic packets on its own interfaces.

use std::fmt;
use std::str::FromStr;

use serde::Deserialize;

use crate::{Error, Result, Session};

/// A saved Wake-on-LAN target.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct WolDevice {
    pub mac: String,
    pub ip: String,
    pub name: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct WolList {
    devices: Vec<WolDevice>,
}

/// A MAC address, accepted as `aa:bb:cc:dd:ee:ff`, `aa-bb-…` or `aabbccddeeff`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mac(pub [u8; 6]);

impl FromStr for Mac {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let hex: String = s
            .chars()
            .filter(|c| !matches!(c, ':' | '-' | '.'))
            .collect();
        let bad = || Error::Protocol(format!("invalid MAC address {s:?}"));
        if hex.len() != 12 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(bad());
        }
        let mut out = [0u8; 6];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| bad())?;
        }
        Ok(Mac(out))
    }
}

impl fmt::Display for Mac {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d, e, g] = self.0;
        write!(f, "{a:02x}:{b:02x}:{c:02x}:{d:02x}:{e:02x}:{g:02x}")
    }
}

impl Session {
    /// Saved Wake-on-LAN targets.
    pub async fn wol_list(&self) -> Result<Vec<WolDevice>> {
        Ok(self.get::<WolList>("wol/list", &[]).await?.devices)
    }

    /// Sends a magic packet to `mac`.
    pub async fn wol_wake(&self, mac: Mac) -> Result<()> {
        let _: serde_json::Value = self.post("wol/wake", &[("mac", &mac.to_string())]).await?;
        Ok(())
    }

    /// Adds or updates a saved target (**device-wide**: shared with other users).
    pub async fn wol_add(&self, mac: Mac, ip: &str, name: &str) -> Result<()> {
        let mac = mac.to_string();
        let _: serde_json::Value = self
            .post("wol/add", &[("mac", &mac), ("ip", ip), ("name", name)])
            .await?;
        Ok(())
    }

    /// Removes a saved target (**device-wide**).
    pub async fn wol_remove(&self, mac: Mac) -> Result<()> {
        let _: serde_json::Value = self
            .post("wol/remove", &[("mac", &mac.to_string())])
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::fixtures;

    #[test]
    fn mac_formats() {
        let want = Mac([0xaa, 0xbb, 0x0c, 0xdd, 0xee, 0x01]);
        for s in [
            "aa:bb:0c:dd:ee:01",
            "AA-BB-0C-DD-EE-01",
            "aabb0cddee01",
            "aabb.0cdd.ee01",
        ] {
            assert_eq!(s.parse::<Mac>().unwrap(), want, "{s}");
        }
        assert_eq!(want.to_string(), "aa:bb:0c:dd:ee:01");
    }

    #[test]
    fn invalid_macs() {
        for s in [
            "",
            "aa:bb:cc:dd:ee",
            "aa:bb:cc:dd:ee:ff:00",
            "gg:bb:cc:dd:ee:ff",
            "aa:bb:cc:dd:ee:f+",
        ] {
            assert!(s.parse::<Mac>().is_err(), "{s}");
        }
    }

    #[test]
    fn parses_device_list() {
        let list: WolList = fixtures::load("wol_list");
        assert!(list.devices.is_empty());
        let full: WolList = serde_json::from_str(
            r#"{"devices":[{"ip":"10.0.0.5","mac":"aa:bb:cc:dd:ee:ff","name":"nas"}]}"#,
        )
        .unwrap();
        assert_eq!(full.devices[0].name, "nas");
    }
}
