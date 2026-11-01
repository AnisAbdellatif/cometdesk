//! Clipboard paste delivery via the target agent or key-by-key HID typing.
//! Entry points: `Console::request_type_clipboard`, `Console::deliver_text`, `Console::start_typing`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use cometdesk::api::hid::PrintOptions;
use cometdesk_clip::SessionState;

use super::state::{Confirm, Typing};
use super::{Console, KEY_GAP};
use crate::actions;
use crate::app::UserEvent;
use crate::clipsync::ClipBridge;
use crate::store::TargetOs;

impl Console {
    pub fn request_type_clipboard(&mut self) {
        let text = match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(t) if !t.is_empty() => t,
            Ok(_) => return self.notify("The clipboard has no text".into()),
            Err(e) => return self.notify(format!("Cannot read the clipboard: {e}")),
        };
        // The user may still hold Cmd+Shift from the shortcut; those must not reach the target
        // together with the typed text.
        self.release_all();
        let locked = self.last_session == Some(SessionState::Locked);
        let reason = actions::confirm_reason(&text).or_else(|| {
            locked.then(|| "been aimed at a locked target: it goes to the lock screen".to_string())
        });
        match reason {
            Some(reason) => self.confirm = Some(Confirm::Type { text, reason }),
            None => self.deliver_text(text),
        }
    }

    /// Text for the target: inserted in one step through the agent when it is connected,
    /// otherwise typed key by key over the KVM.
    pub(super) fn deliver_text(&mut self, text: String) {
        let insert = self.clip.as_ref().is_some_and(ClipBridge::can_insert)
            && text.len() <= cometdesk_clip::MAX_INSERT;
        if !insert {
            return self.start_typing(text);
        }
        let paste = self.device.insert_paste;
        let rx = self
            .clip
            .as_ref()
            .unwrap()
            .insert(text, paste, self.hid.clone());
        self.inserting = Some(rx);
    }

    /// Whether the clipboard button inserts through the agent (vs typing).
    pub(super) fn inserts_via_agent(&self) -> bool {
        self.clip.as_ref().is_some_and(ClipBridge::can_insert)
    }

    /// Types `text` key by key. With a layout table for the target (`cometdesk::layout`) we send
    /// the keys ourselves, AltGr as Right Alt alone (Option on macOS); otherwise the device
    /// types it (`hid/print`), whose AltGr Linux and macOS misread.
    pub(super) fn start_typing(&mut self, text: String) {
        if let Some(t) = self.typing.take() {
            t.task.abort();
        }
        let total = text.chars().count();
        let done = Arc::new(AtomicUsize::new(0));
        let (proxy, progress) = (self.proxy.clone(), done.clone());
        let task = if let Some(layout) = self.typing_layout() {
            let missing = layout.missing(&text);
            if !missing.is_empty() {
                let shown: String = missing.iter().take(10).collect();
                self.notify(format!(
                    "Skipping {} characters the {} layout cannot type: {shown}",
                    missing.len(),
                    layout.name
                ));
            }
            let hid = self.hid.clone();
            self.rt.spawn(async move {
                for (i, ch) in text.chars().enumerate() {
                    for cmd in layout.keys(ch).unwrap_or_default() {
                        hid.send(cmd);
                        tokio::time::sleep(KEY_GAP).await;
                    }
                    if i % 20 == 19 {
                        progress.store(i + 1, Ordering::Relaxed);
                        let _ = proxy.send_event(UserEvent::Refresh);
                    }
                }
                progress.store(total, Ordering::Relaxed);
                Ok(())
            })
        } else {
            let chunks: Vec<String> = actions::chunks(&text, actions::CHUNK_CHARS)
                .into_iter()
                .map(str::to_owned)
                .collect();
            let (session, keymap) = (self.session.clone(), self.keymap());
            self.rt.spawn(async move {
                for chunk in chunks {
                    let opts = PrintOptions {
                        keymap: keymap.as_deref(),
                        slow: false,
                    };
                    session
                        .hid_print(&chunk, opts)
                        .await
                        .map_err(|e| e.to_string())?;
                    progress.fetch_add(chunk.chars().count(), Ordering::Relaxed);
                    let _ = proxy.send_event(UserEvent::Refresh);
                }
                Ok(())
            })
        };
        self.typing = Some(Typing { total, done, task });
    }

    /// Our layout table for typing on this target, if we have one. Firmware screens read
    /// raw US scancodes.
    fn typing_layout(&self) -> Option<cometdesk::layout::Layout> {
        use cometdesk::layout::{Platform, layout};
        match self.device.target {
            TargetOs::Bios => layout("en-us", Platform::Windows),
            TargetOs::MacOs => layout(&self.keymap()?, Platform::Mac),
            TargetOs::Windows | TargetOs::Linux => layout(&self.keymap()?, Platform::Windows),
        }
    }

    /// Layout used to type text: the device's saved choice, else the device default.
    pub(super) fn keymap(&self) -> Option<String> {
        self.device
            .keymap
            .clone()
            .or_else(|| self.default_keymap.clone())
    }

    /// Stops typing: dropping the request makes the device cancel and clear queued keys;
    /// keys we send ourselves may be held mid-character, so release them.
    pub(super) fn cancel_typing(&mut self) {
        if let Some(t) = self.typing.take() {
            t.task.abort();
            self.hid.release_all();
            self.notify("Typing cancelled".into());
        }
    }
}
