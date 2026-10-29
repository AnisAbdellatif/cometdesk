//! Inserting text at the target's focus, in one step instead of a stream of key presses.
//!
//! - Windows: one `SendInput` call carrying the whole text as Unicode characters. The input
//!   queue takes it at once, layout and AltGr don't matter, and the clipboard is untouched.
//! - Elsewhere (and if Windows refuses, e.g. for an elevated window): put the text on the
//!   clipboard and ask the GUI for one paste keystroke over the KVM. Wayland doesn't let
//!   programs fake input, but a real keyboard (the KVM) may paste. The clipboard thread
//!   restores the previous content afterwards.

use std::sync::mpsc;
use std::time::Duration;

use glkvm_clip::{InsertReply, PasteKey, PastePref};

use crate::clipboard::ClipCommand;

pub fn insert(text: &str, paste: PastePref, clip: &mpsc::Sender<ClipCommand>) -> InsertReply {
    #[cfg(windows)]
    if windows::send_unicode(text) {
        return InsertReply::Inserted;
    }
    let (done_tx, done_rx) = mpsc::channel();
    if clip
        .send(ClipCommand::SetForInsert {
            text: text.to_owned(),
            done: done_tx,
        })
        .is_err()
    {
        return InsertReply::Failed("clipboard thread stopped".into());
    }
    match done_rx.recv_timeout(Duration::from_secs(3)) {
        Ok(Ok(())) => InsertReply::PasteNow(paste_key(paste)),
        Ok(Err(e)) => InsertReply::Failed(format!("cannot set the clipboard: {e}")),
        Err(_) => InsertReply::Failed("the clipboard did not answer".into()),
    }
}

fn paste_key(pref: PastePref) -> PasteKey {
    if cfg!(target_os = "macos") {
        return PasteKey::CmdV;
    }
    if cfg!(windows) {
        return PasteKey::CtrlV;
    }
    match pref {
        PastePref::CtrlV => PasteKey::CtrlV,
        PastePref::CtrlShiftV => PasteKey::CtrlShiftV,
        PastePref::Auto => {
            if crate::info::focus().is_some_and(|f| f.terminal) {
                PasteKey::CtrlShiftV
            } else {
                PasteKey::CtrlV
            }
        }
    }
}

#[cfg(windows)]
mod windows {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput,
        VK_RETURN, VK_TAB,
    };

    fn key(vk: u16, scan: u16, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// Queues the whole text as Unicode key events in one call. False if Windows refused
    /// some of them (UIPI blocks input to elevated windows).
    pub fn send_unicode(text: &str) -> bool {
        let mut inputs = Vec::with_capacity(text.len() * 2);
        for ch in text.chars() {
            match ch {
                '\r' => {}
                '\n' => inputs.extend([key(VK_RETURN, 0, 0), key(VK_RETURN, 0, KEYEVENTF_KEYUP)]),
                '\t' => inputs.extend([key(VK_TAB, 0, 0), key(VK_TAB, 0, KEYEVENTF_KEYUP)]),
                _ => {
                    let mut units = [0u16; 2];
                    for &u in ch.encode_utf16(&mut units).iter() {
                        inputs.push(key(0, u, KEYEVENTF_UNICODE));
                        inputs.push(key(0, u, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
                    }
                }
            }
        }
        if inputs.is_empty() {
            return true;
        }
        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
        sent as usize == inputs.len()
    }
}
