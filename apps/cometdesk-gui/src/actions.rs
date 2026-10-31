//! Console actions that are pure logic: special-key combos, typing clipboard text, and the
//! app's own shortcuts.

use cometdesk::hid::HidCommand;
use winit::keyboard::KeyCode;

use crate::store::TargetOs;

/// Key combos for the special-keys menu: OS-reserved on the client, so they can't be typed.
pub const COMBOS: &[(&str, &[&str])] = &[
    ("Ctrl+Alt+Del", &["ControlLeft", "AltLeft", "Delete"]),
    ("Win / Super", &["MetaLeft"]),
    ("Alt+Tab", &["AltLeft", "Tab"]),
    ("Alt+F4", &["AltLeft", "F4"]),
    ("Ctrl+Esc", &["ControlLeft", "Escape"]),
    ("Print Screen", &["PrintScreen"]),
    ("Ctrl+Alt+F1 (Linux TTY)", &["ControlLeft", "AltLeft", "F1"]),
    ("Ctrl+Alt+F2", &["ControlLeft", "AltLeft", "F2"]),
    ("Cmd+Space (macOS)", &["MetaLeft", "Space"]),
    ("Cmd+Tab (macOS)", &["MetaLeft", "Tab"]),
];

/// Function keys, which macOS laptops often reserve for media controls.
pub const FUNCTION_KEYS: [&str; 12] = [
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
];

/// Press all keys in order, release in reverse.
pub fn combo(keys: &[&'static str]) -> Vec<HidCommand> {
    let press = keys.iter().map(|&code| HidCommand::Key {
        code,
        pressed: true,
    });
    let release = keys.iter().rev().map(|&code| HidCommand::Key {
        code,
        pressed: false,
    });
    press.chain(release).collect()
}

/// Text longer than this asks for confirmation before typing.
pub const CONFIRM_CHARS: usize = 1000;
/// Characters per `hid/print` request, so progress can be shown and typing cancelled.
pub const CHUNK_CHARS: usize = 500;

/// Why typing `text` needs a confirmation, if it does.
pub fn confirm_reason(text: &str) -> Option<String> {
    let lines = text.trim_end_matches('\n').lines().count();
    let chars = text.chars().count();
    let mut reasons = Vec::new();
    if lines > 1 {
        reasons.push(format!("{lines} lines (each newline presses Enter)"));
    }
    if chars > CONFIRM_CHARS {
        reasons.push(format!("{chars} characters"));
    }
    let odd = untypeable(text);
    if odd > 0 {
        reasons.push(format!(
            "{odd} characters that may not exist on the target keyboard"
        ));
    }
    (!reasons.is_empty()).then(|| reasons.join(", "))
}

/// Characters outside printable ASCII, tab and newline. The device types characters through
/// the target's keymap and silently skips what it can't produce; without keymap tables we
/// can only flag the likely ones.
pub fn untypeable(text: &str) -> usize {
    text.chars()
        .filter(|c| !matches!(c, ' '..='~' | '\n' | '\t'))
        .count()
}

/// Splits into chunks of at most `max` characters, preferring to break after a newline.
pub fn chunks(text: &str, max: usize) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let end = rest.char_indices().nth(max).map_or(rest.len(), |(i, _)| i);
        if end == rest.len() {
            out.push(rest);
            break;
        }
        let cut = rest[..end].rfind('\n').map_or(end, |i| i + 1);
        out.push(&rest[..cut]);
        rest = &rest[cut..];
    }
    out
}

/// The app's own shortcuts. Right Cmd is the host key: it is never sent to the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    TypeClipboard,
    ToggleFullscreen,
    ToggleStats,
    /// Show the toolbar (e.g. in fullscreen).
    ShowToolbar,
    /// Open the action search.
    Palette,
}

/// Modifier keys held on the host, tracked from raw key events.
#[derive(Debug, Default, Clone, Copy)]
pub struct HostModifiers {
    pub host_key: bool,
    pub cmd_left: bool,
    pub shift: bool,
}

impl HostModifiers {
    pub fn update(&mut self, code: KeyCode, pressed: bool) {
        match code {
            KeyCode::SuperRight => self.host_key = pressed,
            KeyCode::SuperLeft => self.cmd_left = pressed,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => self.shift = pressed,
            _ => {}
        }
    }
}

pub fn is_host_key(code: KeyCode) -> bool {
    code == KeyCode::SuperRight
}

/// Matches a key press against the app shortcuts.
///
/// - Host key (Right Cmd) + V / F / S / T: type clipboard, fullscreen, stats, toolbar.
/// - Cmd+Shift+V types the clipboard too, except for Linux targets: there Cmd acts as Ctrl,
///   and Ctrl+Shift+V is paste in terminals, so it must reach the target.
pub fn shortcut(code: KeyCode, mods: HostModifiers, target: TargetOs) -> Option<Shortcut> {
    if mods.host_key {
        return match code {
            KeyCode::KeyV => Some(Shortcut::TypeClipboard),
            KeyCode::KeyF => Some(Shortcut::ToggleFullscreen),
            KeyCode::KeyS => Some(Shortcut::ToggleStats),
            KeyCode::KeyT => Some(Shortcut::ShowToolbar),
            KeyCode::KeyK => Some(Shortcut::Palette),
            _ => None,
        };
    }
    if code == KeyCode::KeyV && mods.cmd_left && mods.shift && target != TargetOs::Linux {
        return Some(Shortcut::TypeClipboard);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combo_presses_then_releases_in_reverse() {
        let cmds = combo(&["ControlLeft", "AltLeft", "Delete"]);
        let names: Vec<_> = cmds
            .iter()
            .map(|c| match c {
                HidCommand::Key { code, pressed } => {
                    format!("{}{code}", if *pressed { "+" } else { "-" })
                }
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            names,
            [
                "+ControlLeft",
                "+AltLeft",
                "+Delete",
                "-Delete",
                "-AltLeft",
                "-ControlLeft"
            ]
        );
    }

    #[test]
    fn combos_only_use_device_key_names() {
        // Every name must be one the firmware accepts (same set as cometdesk_view::keymap emits).
        let known: Vec<&str> = [
            "ControlLeft",
            "AltLeft",
            "Delete",
            "MetaLeft",
            "Tab",
            "F1",
            "F2",
            "F4",
            "Escape",
            "PrintScreen",
            "Space",
        ]
        .to_vec();
        for (label, keys) in COMBOS {
            for k in *keys {
                assert!(known.contains(k), "{label}: {k}");
            }
        }
    }

    #[test]
    fn short_single_line_needs_no_confirmation() {
        assert_eq!(confirm_reason("ls -la"), None);
        assert_eq!(
            confirm_reason("ls -la\n"),
            None,
            "a single trailing newline is one line"
        );
    }

    #[test]
    fn multiline_long_and_odd_text_needs_confirmation() {
        assert!(confirm_reason("a\nb").unwrap().contains("2 lines"));
        assert!(
            confirm_reason(&"x".repeat(1001))
                .unwrap()
                .contains("1001 characters")
        );
        assert!(confirm_reason("héllo 🙂").unwrap().contains("2 characters"));
    }

    #[test]
    fn untypeable_counts_non_ascii() {
        assert_eq!(untypeable("plain ASCII\twith tab\n"), 0);
        assert_eq!(untypeable("Grüße"), 2);
    }

    #[test]
    fn chunks_break_after_newlines_and_respect_utf8() {
        assert_eq!(chunks("", 5), Vec::<&str>::new());
        assert_eq!(chunks("abc", 5), ["abc"]);
        assert_eq!(chunks("ab\ncdef\ngh", 6), ["ab\n", "cdef\n", "gh"]);
        assert_eq!(chunks("abcdefgh", 3), ["abc", "def", "gh"]);
        let s = "äöüäöü";
        let parts = chunks(s, 4);
        assert_eq!(parts.concat(), s);
        assert!(parts.iter().all(|p| p.chars().count() <= 4));
    }

    #[test]
    fn host_key_shortcuts() {
        let host = HostModifiers {
            host_key: true,
            ..Default::default()
        };
        assert_eq!(
            shortcut(KeyCode::KeyV, host, TargetOs::Linux),
            Some(Shortcut::TypeClipboard)
        );
        assert_eq!(
            shortcut(KeyCode::KeyF, host, TargetOs::Windows),
            Some(Shortcut::ToggleFullscreen)
        );
        assert_eq!(shortcut(KeyCode::KeyA, host, TargetOs::Windows), None);
        assert!(is_host_key(KeyCode::SuperRight) && !is_host_key(KeyCode::SuperLeft));
    }

    #[test]
    fn cmd_shift_v_is_left_alone_for_linux_terminals() {
        let mods = HostModifiers {
            cmd_left: true,
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            shortcut(KeyCode::KeyV, mods, TargetOs::Windows),
            Some(Shortcut::TypeClipboard)
        );
        assert_eq!(
            shortcut(KeyCode::KeyV, mods, TargetOs::MacOs),
            Some(Shortcut::TypeClipboard)
        );
        assert_eq!(shortcut(KeyCode::KeyV, mods, TargetOs::Linux), None);
        let no_shift = HostModifiers {
            cmd_left: true,
            ..Default::default()
        };
        assert_eq!(shortcut(KeyCode::KeyV, no_shift, TargetOs::Windows), None);
    }

    #[test]
    fn modifier_tracking() {
        let mut m = HostModifiers::default();
        m.update(KeyCode::SuperRight, true);
        m.update(KeyCode::ShiftRight, true);
        assert!(m.host_key && m.shift);
        m.update(KeyCode::SuperRight, false);
        assert!(!m.host_key);
    }
}
