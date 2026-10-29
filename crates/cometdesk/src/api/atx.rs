//! ATX power control (`/api/atx`). Needs the ATX board wired to the target.

use serde::Deserialize;

use crate::{Result, Session};

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct AtxState {
    /// False when no ATX board is configured; power actions then fail.
    pub enabled: bool,
    pub busy: bool,
    pub leds: AtxLeds,
    /// `"on"` / `"off"` as reported by the power LED.
    pub power: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct AtxLeds {
    pub power: bool,
    pub hdd: bool,
}

/// Power actions; the device decides the button timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    /// Short press if the target is off.
    On,
    /// Short press if the target is on (graceful shutdown).
    Off,
    /// Long press if the target is on (forced off).
    OffHard,
    /// Reset button if the target is on.
    ResetHard,
}

impl PowerAction {
    pub fn as_str(self) -> &'static str {
        match self {
            PowerAction::On => "on",
            PowerAction::Off => "off",
            PowerAction::OffHard => "off_hard",
            PowerAction::ResetHard => "reset_hard",
        }
    }
}

/// Raw button presses, regardless of the power state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtxButton {
    Power,
    PowerLong,
    Reset,
}

impl AtxButton {
    pub fn as_str(self) -> &'static str {
        match self {
            AtxButton::Power => "power",
            AtxButton::PowerLong => "power_long",
            AtxButton::Reset => "reset",
        }
    }
}

fn wait_arg(wait: bool) -> &'static str {
    if wait { "1" } else { "0" }
}

impl Session {
    pub async fn atx_state(&self) -> Result<AtxState> {
        self.get("atx", &[]).await
    }

    /// Changes the target's power state. With `wait`, returns after the press completes.
    pub async fn atx_power(&self, action: PowerAction, wait: bool) -> Result<()> {
        let _: serde_json::Value = self
            .post(
                "atx/power",
                &[("action", action.as_str()), ("wait", wait_arg(wait))],
            )
            .await?;
        Ok(())
    }

    /// Presses an ATX button.
    pub async fn atx_click(&self, button: AtxButton, wait: bool) -> Result<()> {
        let _: serde_json::Value = self
            .post(
                "atx/click",
                &[("button", button.as_str()), ("wait", wait_arg(wait))],
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::fixtures;

    #[test]
    fn parses_device_state() {
        let s: AtxState = fixtures::load("atx");
        assert!(!s.enabled);
        assert_eq!(s.power.as_deref(), Some("off"));
        assert_eq!(
            s.leds,
            AtxLeds {
                power: false,
                hdd: false
            }
        );
    }

    #[test]
    fn wire_names() {
        let actions = [
            PowerAction::On,
            PowerAction::Off,
            PowerAction::OffHard,
            PowerAction::ResetHard,
        ];
        assert_eq!(
            actions.map(PowerAction::as_str),
            ["on", "off", "off_hard", "reset_hard"]
        );
        let buttons = [AtxButton::Power, AtxButton::PowerLong, AtxButton::Reset];
        assert_eq!(
            buttons.map(AtxButton::as_str),
            ["power", "power_long", "reset"]
        );
    }
}
