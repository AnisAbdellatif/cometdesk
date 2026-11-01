//! Public protocol data types exchanged between `cometdesk-gui` and `cometdesk-agent`.
//! Owns `Hello`, `Info`, `Listing`, `PowerAction`, `Policy`, `PastePref`, and `Purpose`.

use serde::{Deserialize, Serialize};

use crate::limits::{MAX_LISTING_BYTES, PROTOCOL};

/// What the receiver does with a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Purpose {
    /// Keep it in the Downloads folder.
    Save,
    /// Stage it, and put it on the clipboard with the rest of its batch on `FilesReady`.
    Clipboard,
    /// A new agent program (GUI → agent): replace the running one and restart.
    Update,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// cometdesk-gui: Noise initiator.
    Client,
    /// cometdesk-agent on the target: Noise responder.
    Agent,
}

/// What each end tells the other once the session is up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub protocol: u16,
    /// `macos`, `linux`, `windows`.
    pub os: String,
    /// Host name of the machine, for display.
    pub name: String,
    /// Software version (`CARGO_PKG_VERSION`).
    pub version: String,
    /// `x86_64`, `aarch64`: picks the agent program an update installs.
    pub arch: String,
}

impl Hello {
    pub fn local(version: &str, name: String) -> Hello {
        Hello {
            protocol: PROTOCOL,
            os: std::env::consts::OS.to_owned(),
            name,
            version: version.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
        }
    }
}

/// What the agent knows about the target, sent after `Hello` and whenever it changes.
/// Every field is best effort: `None` or empty when the OS does not say.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Info {
    /// Active keyboard layout, tagged by source: `xkb:de+nodeadkeys`, `win:0407`,
    /// `mac:com.apple.keylayout.German`. The GUI maps it to a device keymap.
    pub keyboard: Option<String>,
    /// Every layout installed on the target, same tagging; the active one is not marked.
    pub layouts: Vec<String>,
    /// OS name and version for display (`Ubuntu 24.04 LTS`, `macOS 15.1`).
    pub os_version: Option<String>,
    pub session: SessionState,
    /// The monitors of the target, in desktop coordinates.
    pub displays: Vec<Display>,
    /// An input method editor (CJK, …) is on: key-by-key typing may be composed into
    /// something else.
    pub ime: Option<bool>,
    /// What has the keyboard focus. Only the application, never window titles.
    pub focus: Option<Focus>,
    /// Account the agent runs as.
    pub user: Option<String>,
    /// Seconds since the target booted. Not compared when deciding whether to resend.
    pub uptime_secs: Option<u64>,
    pub battery: Option<Battery>,
    /// Load, memory and system-drive use. Moves constantly: not compared when deciding
    /// whether to resend (the agent resends it on a timer instead).
    pub load: Option<Load>,
    /// Master output volume, if the OS says.
    pub volume: Option<Volume>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Volume {
    pub percent: u8,
    pub muted: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    #[default]
    Unknown,
    Active,
    /// Lock screen or secure desktop: typed text goes to a password field, not an app.
    Locked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Display {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Percent: 100, 125, 150, 200.
    pub scale: u32,
    pub primary: bool,
    /// Resolutions the display accepts (width, height), largest first; empty if unknown.
    pub modes: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Focus {
    /// Executable or window class (`notepad.exe`, `gnome-terminal-server`).
    pub app: String,
    pub terminal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
}

/// Resource use of the target.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Load {
    /// CPU busy over the last poll, 0-100; `None` until two samples exist.
    pub cpu_percent: Option<u8>,
    pub memory: Option<Usage>,
    /// The system drive (`C:`, `/`).
    pub disk: Option<Usage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub used_mb: u64,
    pub total_mb: u64,
}

impl Info {
    /// Equality for deciding whether to resend: uptime and load always move.
    pub fn same_as(&self, other: &Info) -> bool {
        let mut a = self.clone();
        a.uptime_secs = other.uptime_secs;
        a.load = other.load;
        a == *other
    }
}

/// What the GUI can ask the agent to do to the target's session or power state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerAction {
    Lock,
    Sleep,
    Logoff,
    Restart,
    Shutdown,
}

impl PowerAction {
    pub fn label(self) -> &'static str {
        match self {
            PowerAction::Lock => "Lock",
            PowerAction::Sleep => "Sleep",
            PowerAction::Logoff => "Log off",
            PowerAction::Restart => "Restart",
            PowerAction::Shutdown => "Shut down",
        }
    }

    /// Loses the user's unsaved work or the agent connection for good.
    pub fn destructive(self) -> bool {
        matches!(
            self,
            PowerAction::Logoff | PowerAction::Restart | PowerAction::Shutdown
        )
    }
}

/// Which folder of the target's file system to list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirTarget {
    /// The user's home folder.
    Home,
    /// The drives (Windows) or `/`.
    Roots,
    /// A path from an earlier [`Listing`].
    Open(String),
    /// A folder inside `dir` (a path from an earlier listing); `name` is one entry's name.
    Child { dir: String, name: String },
    /// The folder containing `dir`; the drives above a drive's root.
    Parent(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listing {
    /// The folder listed; empty for the list of drives.
    pub path: String,
    /// Folders first, then files, each sorted by name.
    pub entries: Vec<DirEntry>,
    /// More entries existed than fit one message.
    pub truncated: bool,
}

impl Listing {
    /// Cuts `entries` to what fits one message.
    pub fn fit(mut self) -> Listing {
        let mut budget = MAX_LISTING_BYTES.saturating_sub(self.path.len());
        let keep = self
            .entries
            .iter()
            .take_while(|e| {
                let cost = e.name.len() + 12;
                budget = budget.saturating_sub(cost);
                budget > 0
            })
            .count();
        if keep < self.entries.len() {
            self.entries.truncate(keep);
            self.truncated = true;
        }
        self
    }
}

/// Sent by the GUI; the agent must not send its clipboard unless allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    pub agent_may_send: bool,
}

/// Which keystroke pastes on the target, as the GUI prefers it for Linux targets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PastePref {
    /// The agent decides (terminals get Ctrl+Shift+V where it can tell).
    #[default]
    Auto,
    CtrlV,
    CtrlShiftV,
}

/// A paste keystroke the GUI sends over the KVM's keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasteKey {
    CtrlV,
    CtrlShiftV,
    CmdV,
}

/// The agent's answer to an insert request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsertReply {
    /// Inserted directly (Windows: one `SendInput` with Unicode characters).
    Inserted,
    /// The text is on the target's clipboard: send this keystroke, then
    /// [`Endpoint::send_insert_pasted`] so the agent restores the previous clipboard.
    PasteNow(PasteKey),
    Failed(String),
}
