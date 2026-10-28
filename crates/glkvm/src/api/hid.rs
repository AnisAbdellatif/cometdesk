//! HID state, keymaps and text typing (`/api/hid`). Live input goes over `/api/ws`
//! (see [`crate::events`]).

use serde::Deserialize;

use crate::client::{Body, parse_envelope};
use crate::{Result, Session};

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct HidState {
    pub enabled: bool,
    /// The target has enumerated the USB HID devices (it is powered and booted far enough).
    pub online: bool,
    pub connected: bool,
    pub busy: bool,
    pub keyboard: Keyboard,
    pub mouse: Mouse,
    pub jiggler: Jiggler,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Keyboard {
    pub online: bool,
    pub leds: KeyboardLeds,
    pub outputs: Outputs,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct KeyboardLeds {
    pub caps: bool,
    pub num: bool,
    pub scroll: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Mouse {
    pub online: bool,
    pub absolute: bool,
    pub outputs: Outputs,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Outputs {
    pub active: String,
    pub available: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Jiggler {
    pub active: bool,
    pub enabled: bool,
    pub interval: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Keymaps {
    pub default: String,
    pub available: Vec<String>,
}

#[derive(Deserialize)]
struct KeymapsResult {
    keymaps: Keymaps,
}

#[derive(Debug, Clone, Default)]
pub struct PrintOptions<'a> {
    /// Target keyboard layout (from [`Session::hid_keymaps`]); the device default if `None`.
    pub keymap: Option<&'a str>,
    /// Type slowly, for BIOS/UEFI and other slow input handlers.
    pub slow: bool,
}

impl Session {
    pub async fn hid_state(&self) -> Result<HidState> {
        self.get("hid", &[]).await
    }

    pub async fn hid_keymaps(&self) -> Result<Keymaps> {
        Ok(self.get::<KeymapsResult>("hid/keymaps", &[]).await?.keymaps)
    }

    /// Types `text` on the target as key presses (newlines become Enter). Characters the
    /// keymap can't produce are skipped by the device. Returns when typing is done.
    pub async fn hid_print(&self, text: &str, opts: PrintOptions<'_>) -> Result<()> {
        // limit=0: the device would otherwise truncate to 1024 characters.
        let mut query = vec![("limit", "0"), ("slow", if opts.slow { "1" } else { "0" })];
        if let Some(keymap) = opts.keymap {
            query.push(("keymap", keymap));
        }
        let (status, body) = self
            .request(reqwest::Method::POST, "hid/print", &query, Body::Text(text))
            .await?;
        let _: serde_json::Value = parse_envelope(status, &body)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::fixtures;

    #[test]
    fn parses_hid_state() {
        let s: HidState = fixtures::load("hid");
        assert!(s.enabled && s.online && s.keyboard.online && s.mouse.online);
        assert!(s.mouse.absolute);
        assert_eq!(s.mouse.outputs.active, "usb");
        assert!(s.mouse.outputs.available.iter().any(|o| o == "usb_rel"));
        assert!(s.keyboard.leds.num);
    }

    #[test]
    fn parses_keymaps() {
        let k: KeymapsResult = fixtures::load("hid_keymaps");
        assert_eq!(k.keymaps.default, "en-us");
        assert!(k.keymaps.available.iter().any(|m| m == "de"));
    }
}
