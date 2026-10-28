//! winit physical key → W3C `KeyboardEvent.code` name accepted by the device.
//!
//! Written from the W3C UI Events KeyboardEvent code spec. Only codes the device knows are
//! mapped; anything else returns `None` and is not sent.

use winit::keyboard::KeyCode;

pub fn code_name(code: KeyCode) -> Option<&'static str> {
    use KeyCode::*;
    Some(match code {
        KeyA => "KeyA",
        KeyB => "KeyB",
        KeyC => "KeyC",
        KeyD => "KeyD",
        KeyE => "KeyE",
        KeyF => "KeyF",
        KeyG => "KeyG",
        KeyH => "KeyH",
        KeyI => "KeyI",
        KeyJ => "KeyJ",
        KeyK => "KeyK",
        KeyL => "KeyL",
        KeyM => "KeyM",
        KeyN => "KeyN",
        KeyO => "KeyO",
        KeyP => "KeyP",
        KeyQ => "KeyQ",
        KeyR => "KeyR",
        KeyS => "KeyS",
        KeyT => "KeyT",
        KeyU => "KeyU",
        KeyV => "KeyV",
        KeyW => "KeyW",
        KeyX => "KeyX",
        KeyY => "KeyY",
        KeyZ => "KeyZ",
        Digit0 => "Digit0",
        Digit1 => "Digit1",
        Digit2 => "Digit2",
        Digit3 => "Digit3",
        Digit4 => "Digit4",
        Digit5 => "Digit5",
        Digit6 => "Digit6",
        Digit7 => "Digit7",
        Digit8 => "Digit8",
        Digit9 => "Digit9",
        Enter => "Enter",
        Escape => "Escape",
        Backspace => "Backspace",
        Tab => "Tab",
        Space => "Space",
        Minus => "Minus",
        Equal => "Equal",
        BracketLeft => "BracketLeft",
        BracketRight => "BracketRight",
        Backslash => "Backslash",
        Semicolon => "Semicolon",
        Quote => "Quote",
        Backquote => "Backquote",
        Comma => "Comma",
        Period => "Period",
        Slash => "Slash",
        CapsLock => "CapsLock",
        F1 => "F1",
        F2 => "F2",
        F3 => "F3",
        F4 => "F4",
        F5 => "F5",
        F6 => "F6",
        F7 => "F7",
        F8 => "F8",
        F9 => "F9",
        F10 => "F10",
        F11 => "F11",
        F12 => "F12",
        F20 => "F20",
        PrintScreen => "PrintScreen",
        ScrollLock => "ScrollLock",
        Pause => "Pause",
        Insert => "Insert",
        Home => "Home",
        PageUp => "PageUp",
        Delete => "Delete",
        End => "End",
        PageDown => "PageDown",
        ArrowRight => "ArrowRight",
        ArrowLeft => "ArrowLeft",
        ArrowDown => "ArrowDown",
        ArrowUp => "ArrowUp",
        ControlLeft => "ControlLeft",
        ShiftLeft => "ShiftLeft",
        AltLeft => "AltLeft",
        SuperLeft => "MetaLeft",
        ControlRight => "ControlRight",
        ShiftRight => "ShiftRight",
        AltRight => "AltRight",
        SuperRight => "MetaRight",
        ContextMenu => "ContextMenu",
        NumLock => "NumLock",
        NumpadDivide => "NumpadDivide",
        NumpadMultiply => "NumpadMultiply",
        NumpadSubtract => "NumpadSubtract",
        NumpadAdd => "NumpadAdd",
        NumpadEnter => "NumpadEnter",
        NumpadDecimal => "NumpadDecimal",
        Numpad0 => "Numpad0",
        Numpad1 => "Numpad1",
        Numpad2 => "Numpad2",
        Numpad3 => "Numpad3",
        Numpad4 => "Numpad4",
        Numpad5 => "Numpad5",
        Numpad6 => "Numpad6",
        Numpad7 => "Numpad7",
        Numpad8 => "Numpad8",
        Numpad9 => "Numpad9",
        Power => "Power",
        IntlBackslash => "IntlBackslash",
        IntlYen => "IntlYen",
        IntlRo => "IntlRo",
        KanaMode => "KanaMode",
        Convert => "Convert",
        NonConvert => "NonConvert",
        AudioVolumeMute => "AudioVolumeMute",
        AudioVolumeUp => "AudioVolumeUp",
        AudioVolumeDown => "AudioVolumeDown",
        // Mac "Help" sits where PC keyboards have Insert.
        Help => "Insert",
        _ => return None,
    })
}

/// Per-session key translation options.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyOptions {
    /// macOS reports the two ISO-only keys swapped: the key left of `1` arrives as
    /// `IntlBackslash` and the key right of left Shift as `Backquote`. Swap them back to
    /// physical positions.
    pub iso_fix: bool,
    /// Send Cmd as Ctrl and Ctrl as Cmd (Mac client → Windows/Linux target).
    pub swap_cmd_ctrl: bool,
}

pub fn translate(code: KeyCode, opts: KeyOptions) -> Option<&'static str> {
    use KeyCode::*;
    let code = match code {
        IntlBackslash if opts.iso_fix => Backquote,
        Backquote if opts.iso_fix => IntlBackslash,
        SuperLeft if opts.swap_cmd_ctrl => ControlLeft,
        SuperRight if opts.swap_cmd_ctrl => ControlRight,
        ControlLeft if opts.swap_cmd_ctrl => SuperLeft,
        ControlRight if opts.swap_cmd_ctrl => SuperRight,
        c => c,
    };
    code_name(code)
}

pub fn is_meta(name: &str) -> bool {
    matches!(name, "MetaLeft" | "MetaRight")
}

pub fn is_modifier(name: &str) -> bool {
    matches!(
        name,
        "ControlLeft"
            | "ControlRight"
            | "ShiftLeft"
            | "ShiftRight"
            | "AltLeft"
            | "AltRight"
            | "MetaLeft"
            | "MetaRight"
    )
}

/// Whether the current macOS keyboard is an ISO layout.
#[cfg(target_os = "macos")]
pub fn host_keyboard_is_iso() -> bool {
    #[link(name = "Carbon", kind = "framework")]
    unsafe extern "C" {
        fn LMGetKbdType() -> u8;
        fn KBGetLayoutType(keyboard_type: i16) -> u32;
    }
    const ISO: u32 = u32::from_be_bytes(*b"ISO ");
    // SAFETY: plain Carbon queries without arguments that need validation.
    unsafe { KBGetLayoutType(i16::from(LMGetKbdType())) == ISO }
}

#[cfg(not(target_os = "macos"))]
pub fn host_keyboard_is_iso() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use KeyCode::*;
    use std::collections::HashSet;

    const SAMPLE: &[KeyCode] = &[
        KeyA,
        KeyZ,
        Digit0,
        Enter,
        Backquote,
        IntlBackslash,
        SuperLeft,
        SuperRight,
        F12,
        F20,
        NumpadEnter,
        ArrowUp,
        Help,
        CapsLock,
    ];

    #[test]
    fn names_are_wire_safe() {
        for &c in SAMPLE {
            let n = code_name(c).unwrap();
            assert!(n.is_ascii() && !n.is_empty() && n.len() <= 32, "{c:?}");
        }
    }

    #[test]
    fn super_is_meta() {
        assert_eq!(code_name(SuperLeft), Some("MetaLeft"));
        assert_eq!(code_name(SuperRight), Some("MetaRight"));
    }

    #[test]
    fn unsupported_keys_are_dropped() {
        assert_eq!(code_name(F13), None);
        assert_eq!(code_name(Fn), None);
        assert_eq!(code_name(NumpadEqual), None);
    }

    #[test]
    fn iso_fix_swaps_only_the_two_iso_keys() {
        let iso = KeyOptions {
            iso_fix: true,
            ..Default::default()
        };
        assert_eq!(translate(IntlBackslash, iso), Some("Backquote"));
        assert_eq!(translate(Backquote, iso), Some("IntlBackslash"));
        assert_eq!(translate(KeyA, iso), Some("KeyA"));
        assert_eq!(
            translate(IntlBackslash, KeyOptions::default()),
            Some("IntlBackslash")
        );
    }

    #[test]
    fn swap_cmd_ctrl() {
        let o = KeyOptions {
            swap_cmd_ctrl: true,
            ..Default::default()
        };
        assert_eq!(translate(SuperLeft, o), Some("ControlLeft"));
        assert_eq!(translate(ControlRight, o), Some("MetaRight"));
        assert_eq!(translate(AltLeft, o), Some("AltLeft"));
    }

    #[test]
    fn modifier_classification() {
        assert!(is_meta("MetaLeft") && !is_meta("ControlLeft"));
        assert!(is_modifier("ShiftRight") && !is_modifier("KeyA"));
    }

    /// Checks every name we can emit against the firmware's accepted list when its source
    /// is available: `GLKVM_FIRMWARE_SRC=/path/to/glkvm cargo test -p glkvm-cli`.
    #[test]
    fn names_match_firmware_when_available() {
        let Ok(root) = std::env::var("GLKVM_FIRMWARE_SRC") else {
            return;
        };
        let src = std::fs::read_to_string(format!("{root}/kvmd/keyboard/mappings.py")).unwrap();
        let section = src
            .split("WEB_TO_EVDEV")
            .nth(1)
            .unwrap()
            .split('}')
            .next()
            .unwrap();
        let accepted: HashSet<&str> = section
            .lines()
            .filter_map(|l| l.trim().strip_prefix('"')?.split('"').next())
            .collect();
        let all = all_key_codes();
        let mut emitted = 0;
        for c in all {
            if let Some(n) = code_name(c) {
                emitted += 1;
                assert!(accepted.contains(n), "{n} not accepted by firmware");
            }
        }
        assert!(emitted > 100);
    }

    fn all_key_codes() -> Vec<KeyCode> {
        // Exhaustive enough for the check: every variant we map, plus neighbours.
        vec![
            Backquote,
            Backslash,
            BracketLeft,
            BracketRight,
            Comma,
            Digit0,
            Digit1,
            Digit2,
            Digit3,
            Digit4,
            Digit5,
            Digit6,
            Digit7,
            Digit8,
            Digit9,
            Equal,
            IntlBackslash,
            IntlRo,
            IntlYen,
            KeyA,
            KeyB,
            KeyC,
            KeyD,
            KeyE,
            KeyF,
            KeyG,
            KeyH,
            KeyI,
            KeyJ,
            KeyK,
            KeyL,
            KeyM,
            KeyN,
            KeyO,
            KeyP,
            KeyQ,
            KeyR,
            KeyS,
            KeyT,
            KeyU,
            KeyV,
            KeyW,
            KeyX,
            KeyY,
            KeyZ,
            Minus,
            Period,
            Quote,
            Semicolon,
            Slash,
            AltLeft,
            AltRight,
            Backspace,
            CapsLock,
            ContextMenu,
            ControlLeft,
            ControlRight,
            Enter,
            SuperLeft,
            SuperRight,
            ShiftLeft,
            ShiftRight,
            Space,
            Tab,
            Convert,
            KanaMode,
            NonConvert,
            Delete,
            End,
            Help,
            Home,
            Insert,
            PageDown,
            PageUp,
            ArrowDown,
            ArrowLeft,
            ArrowRight,
            ArrowUp,
            NumLock,
            Numpad0,
            Numpad1,
            Numpad2,
            Numpad3,
            Numpad4,
            Numpad5,
            Numpad6,
            Numpad7,
            Numpad8,
            Numpad9,
            NumpadAdd,
            NumpadDecimal,
            NumpadDivide,
            NumpadEnter,
            NumpadMultiply,
            NumpadSubtract,
            Escape,
            PrintScreen,
            ScrollLock,
            Pause,
            AudioVolumeDown,
            AudioVolumeMute,
            AudioVolumeUp,
            Power,
            F1,
            F2,
            F3,
            F4,
            F5,
            F6,
            F7,
            F8,
            F9,
            F10,
            F11,
            F12,
            F13,
            F20,
        ]
    }
}
