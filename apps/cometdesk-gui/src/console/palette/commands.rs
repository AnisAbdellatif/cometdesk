//! Available palette commands and their execution dispatch.
//! Entry points: `Console::commands`, `Console::run_command`.

use cometdesk::api::atx::{AtxButton, PowerAction};
use cometdesk::api::capabilities::Feature;

use super::Cmd;
use crate::actions;
use crate::console::state::{Confirm, Tab};
use crate::console::{Console, ConsoleAction};

impl Console {
    pub(super) fn commands(&self) -> Vec<(String, Cmd)> {
        let agent = self.inserts_via_agent();
        let mut v = vec![("Paste the clipboard".to_string(), Cmd::Paste)];
        for (label, keys) in actions::COMBOS {
            v.push((format!("Send {label}"), Cmd::Keys(keys)));
        }
        if self.caps.supports(Feature::Atx) {
            v.extend([
                ("Power on".into(), Cmd::Power(PowerAction::On)),
                ("Shut down".into(), Cmd::Power(PowerAction::Off)),
                ("Force power off".into(), Cmd::Power(PowerAction::OffHard)),
                ("Reset".into(), Cmd::Power(PowerAction::ResetHard)),
                (
                    "Press the power button".into(),
                    Cmd::Click(AtxButton::Power),
                ),
            ]);
        }
        v.push(("Save device snapshot".into(), Cmd::Snapshot));
        v.push(("Copy screen text (OCR)".into(), Cmd::Ocr));
        if agent {
            v.extend([
                ("Save exact screenshot".into(), Cmd::ExactShot),
                ("Copy region as image".into(), Cmd::CopyRegion),
                ("Copy screenshot as image".into(), Cmd::CopyScreen),
                ("Send file…".into(), Cmd::SendFile),
                ("Browse the target's files…".into(), Cmd::BrowseFiles),
            ]);
        }
        v.extend([
            ("Toggle fullscreen".into(), Cmd::Fullscreen),
            ("Toggle stats".into(), Cmd::Stats),
            ("Open panel: Target".into(), Cmd::Panel(Tab::Target)),
            ("Open panel: Video".into(), Cmd::Panel(Tab::Video)),
            ("Open panel: Settings".into(), Cmd::Panel(Tab::Settings)),
            ("Open the device web interface".into(), Cmd::WebUi),
            ("Back to devices".into(), Cmd::Disconnect),
        ]);
        v
    }

    pub(super) fn run_command(&mut self, cmd: Cmd) -> ConsoleAction {
        match cmd {
            Cmd::Paste => self.request_type_clipboard(),
            Cmd::Keys(keys) => self.send_combo(keys),
            Cmd::Power(a) => self.confirm = Some(Confirm::Power(a)),
            Cmd::Click(b) => self.confirm = Some(Confirm::Click(b)),
            Cmd::Snapshot => self.snapshot(),
            Cmd::ExactShot => self.exact_screenshot(),
            Cmd::CopyRegion => self.pick_region(),
            Cmd::CopyScreen => self.copy_screen_image([0.0, 0.0, 1.0, 1.0]),
            Cmd::Ocr => self.copy_screen_text(),
            Cmd::Fullscreen => return ConsoleAction::ToggleFullscreen,
            Cmd::Stats => self.show_stats = !self.show_stats,
            Cmd::Panel(t) => self.drawer = Some(t),
            Cmd::SendFile => self.pick_files_to_send(),
            Cmd::BrowseFiles => {
                if let Some(clip) = self.clip.as_ref() {
                    self.browser.open(clip);
                }
            }
            Cmd::WebUi => {
                let url = format!("https://{}/", self.session.host());
                if let Err(e) = open::that(&url) {
                    self.notify(format!("Cannot open {url}: {e}"));
                }
            }
            Cmd::Disconnect => return ConsoleAction::Disconnect,
        }
        ConsoleAction::None
    }
}
