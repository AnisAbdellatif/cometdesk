//! Target OS selection, agent telemetry adoption and target installer commands.
//! Entry points: `Console::set_target`, `Console::adopt_agent_os`, `Console::agent`, `install_command`.

use std::time::Instant;

use cometdesk_clip::{Hello, Info, SessionState};

use super::Console;
use crate::clipsync::BridgeState;
use crate::store::TargetOs;

impl Console {
    /// Changes the target OS (modifier mapping, install command) and saves it.
    pub(super) fn set_target(&mut self, os: TargetOs) {
        if self.device.target == os {
            return;
        }
        self.device.target = os;
        let iso = self.mapper.keys.iso_fix;
        self.mapper.keys = os.key_options(iso);
        self.target_changed = true;
    }

    /// The agent reports the target's real OS: correct a wrong (e.g. default) setting.
    pub(super) fn adopt_agent_os(&mut self) {
        let Some((hello, info)) = self.agent() else {
            if std::mem::take(&mut self.agent_was_up) {
                self.last_session = None;
                self.notify("Agent disconnected: the target slept, restarted or logged out".into());
            }
            return;
        };
        self.agent_was_up = true;
        self.track_agent_info(&info);
        self.adopt_agent_keymap(info.keyboard.as_deref());
        let reported = match hello.os.as_str() {
            "linux" => TargetOs::Linux,
            "windows" => TargetOs::Windows,
            "macos" => TargetOs::MacOs,
            _ => return,
        };
        if self.device.target != reported && self.device.target != TargetOs::Bios {
            self.set_target(reported);
            self.notify(format!(
                "Target set to {} (reported by the agent)",
                reported.label()
            ));
        }
    }

    /// The agent's hello and latest report, while it is connected.
    pub(super) fn agent(&self) -> Option<(Hello, Info)> {
        match self.clip.as_ref()?.state() {
            BridgeState::Connected(h, i) => Some((h, i)),
            _ => None,
        }
    }

    fn track_agent_info(&mut self, info: &Info) {
        if let Some(up) = info.uptime_secs
            && self.uptime_seen.is_none_or(|(u, _)| u != up)
        {
            self.uptime_seen = Some((up, Instant::now()));
        }
        let before = self.last_session.replace(info.session);
        match (before, info.session) {
            (_, SessionState::Locked) if before != Some(SessionState::Locked) => {
                self.notify("Target is locked: typed text goes to the lock screen".into());
            }
            (Some(SessionState::Locked), SessionState::Active) => {
                self.notify("Target unlocked".into());
            }
            _ => {}
        }
    }

    /// The agent reports the target's active keyboard layout: use it for typing, once per
    /// change, so a manual choice sticks until the target's layout changes.
    fn adopt_agent_keymap(&mut self, reported: Option<&str>) {
        let Some(reported) = reported else { return };
        if self.adopted_keyboard.as_deref() == Some(reported) {
            return;
        }
        self.adopted_keyboard = Some(reported.to_owned());
        let Some(map) = cometdesk::layout::keymap_from_native(reported) else {
            return;
        };
        if self.keymap().as_deref() != Some(map) && self.keymaps.iter().any(|k| k == map) {
            self.device.keymap = Some(map.to_owned());
            self.target_changed = true;
            self.notify(format!("Layout set to {map} (reported by the agent)"));
        }
    }
}

/// Agent binary the target installer downloads for `os`.
pub(super) fn agent_name(os: TargetOs) -> Option<&'static str> {
    match os {
        TargetOs::Linux => Some("linux-x86_64"),
        TargetOs::Windows => Some("windows-x86_64.exe"),
        TargetOs::MacOs | TargetOs::Bios => None,
    }
}

/// The one command that installs, pairs and starts the agent (bridge/target/install.*).
///
/// It is typed over HID in the target's layout, so it avoids characters that need AltGr on
/// common layouts (`|`, `[`, `]`, `{`, `}`, `\`, `@`, `~` on German, for one): those may not
/// arrive. Hence a download to a file instead of a pipe, and `/` in Windows paths.
pub(super) fn install_command(os: TargetOs, code: &str) -> Option<String> {
    const URL: &str = "http://192.168.5.1:47475";
    match os {
        TargetOs::Linux => Some(format!(
            "curl -fsSo /tmp/cometdesk-install.sh {URL}/install.sh && sh /tmp/cometdesk-install.sh {code}"
        )),
        TargetOs::Windows => Some(format!(
            "powershell -NoProfile -ExecutionPolicy Bypass -Command \"iwr -UseBasicParsing {URL}/install.ps1 -OutFile $env:TEMP/cometdesk-install.ps1; & $env:TEMP/cometdesk-install.ps1 {code}\""
        )),
        TargetOs::MacOs | TargetOs::Bios => None,
    }
}
