//! Action search palette (Right Cmd + K): command definitions, grouping and modal UI.
//! Read `commands.rs` to add or change an action, `view.rs` to change the search modal.

mod commands;
mod view;

use cometdesk::api::atx::{AtxButton, PowerAction};

use super::state::Tab;

/// An entry of the action search.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Cmd {
    Paste,
    Keys(&'static [&'static str]),
    Power(PowerAction),
    Click(AtxButton),
    Snapshot,
    ExactShot,
    CopyRegion,
    CopyScreen,
    Ocr,
    Fullscreen,
    Stats,
    Panel(Tab),
    SendFile,
    BrowseFiles,
    WebUi,
    Disconnect,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CmdGroup {
    Power,
    Viewer,
    Keys,
    Capture,
    Files,
}

impl CmdGroup {
    pub(super) const ALL: [CmdGroup; 5] = [
        CmdGroup::Power,
        CmdGroup::Viewer,
        CmdGroup::Keys,
        CmdGroup::Capture,
        CmdGroup::Files,
    ];

    pub(super) fn label(&self) -> &'static str {
        match self {
            CmdGroup::Power => "Power",
            CmdGroup::Viewer => "Viewer",
            CmdGroup::Keys => "Keys",
            CmdGroup::Capture => "Capture",
            CmdGroup::Files => "Files",
        }
    }
}

impl Cmd {
    pub(super) fn group(&self) -> CmdGroup {
        match self {
            Cmd::Power(_) | Cmd::Click(_) => CmdGroup::Power,
            Cmd::Fullscreen | Cmd::Stats | Cmd::Panel(_) | Cmd::WebUi | Cmd::Disconnect => {
                CmdGroup::Viewer
            }
            Cmd::Paste | Cmd::Keys(_) => CmdGroup::Keys,
            Cmd::Snapshot | Cmd::ExactShot | Cmd::CopyRegion | Cmd::CopyScreen | Cmd::Ocr => {
                CmdGroup::Capture
            }
            Cmd::SendFile | Cmd::BrowseFiles => CmdGroup::Files,
        }
    }

    pub(super) fn is_confirm_gated(&self) -> bool {
        matches!(self, Cmd::Power(_) | Cmd::Click(_))
    }

    pub(super) fn shortcut_text(&self) -> Option<&'static str> {
        match self {
            Cmd::Paste => Some("Right Cmd+V"),
            Cmd::Fullscreen => Some("Right Cmd+F"),
            Cmd::Stats => Some("Right Cmd+S"),
            _ => None,
        }
    }
}
