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

use cometdesk_clip::{InsertReply, PasteKey, PastePref};

use crate::clipboard::ClipCommand;

pub fn insert(text: &str, paste: PastePref, clip: &mpsc::Sender<ClipCommand>) -> InsertReply {
    #[cfg(windows)]
    if desktop_kit::input::type_text(text) {
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
