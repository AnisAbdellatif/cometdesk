//! Clipboard bridge client: syncs this Mac's clipboard with `cometdesk-agent` on the target,
//! through the relay on the Comet (`/cometdesk-bridge/ws`, docs/clipboard-bridge.md).
//!
//! One task per console. It reconnects on its own; every connection is a fresh end-to-end
//! encrypted session (the relay resets both sides when either reconnects). Only changes made
//! while connected are sent, never what was on the clipboard before, and content marked
//! concealed or transient by password managers is never sent.
//!
//! It also inserts text on the target through the agent ([`ClipBridge::insert`]): in one
//! step instead of a stream of key presses that a window switch could misdirect. When the
//! agent asks for a paste keystroke, this task sends it over the KVM's keyboard.

mod bridge;
mod capture;
mod dirlist;
mod files;
mod insert;
mod notes;
mod run;
mod settings;
mod state;
mod supervise;
#[cfg(test)]
mod tests;
mod types;
mod update;

pub use bridge::ClipBridge;
pub use settings::Settings;
pub use state::BridgeState;
#[allow(unused_imports)]
pub use types::{
    AckResult, CaptureResult, FileSend, InsertResult, ListResult, Note, Request, TransferProgress,
};

use cometdesk_clip::{Content, Dedup};

/// This Mac's clipboard: change detection, echo suppression, and the concealed check.
struct LocalClipboard {
    dedup: Dedup,
    last_count: Option<isize>,
}

impl LocalClipboard {
    /// What is on the clipboard now counts as seen: it is never sent.
    async fn new() -> LocalClipboard {
        let mut lc = LocalClipboard {
            dedup: Dedup::default(),
            last_count: Some(pasteboard::change_count()),
        };
        if let Some(c) = read_blocking().await {
            lc.dedup.observe_local(c.key());
        }
        lc
    }

    /// New local content to send, if the clipboard changed and it may be shared.
    pub(super) async fn poll(&mut self) -> Option<Content> {
        let count = pasteboard::change_count();
        if self.last_count == Some(count) {
            return None;
        }
        self.last_count = Some(count);
        if pasteboard::concealed() {
            tracing::info!("clipboard content marked concealed/transient: not shared");
            return None;
        }
        let content = read_blocking().await?;
        self.dedup.observe_local(content.key()).then_some(content)
    }

    pub(super) async fn apply(&mut self, content: Content) {
        self.dedup.applied(content.key());
        let r = tokio::task::spawn_blocking(move || {
            let mut clip = arboard::Clipboard::new()?;
            match content {
                Content::Text(t) => clip.set_text(t),
                Content::Files(paths) => clip.set().file_list(&paths),
                Content::Image {
                    width,
                    height,
                    rgba,
                } => clip.set_image(arboard::ImageData {
                    width: width as usize,
                    height: height as usize,
                    bytes: rgba.into(),
                }),
            }
        })
        .await;
        match r {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!(%e, "cannot set the clipboard"),
            Err(e) => tracing::warn!(%e, "clipboard task failed"),
        }
    }
}

async fn read_blocking() -> Option<Content> {
    tokio::task::spawn_blocking(|| {
        let mut clip = arboard::Clipboard::new().ok()?;
        if let Ok(files) = clip.get().file_list()
            && !files.is_empty()
        {
            return Some(Content::Files(files));
        }
        if let Ok(t) = clip.get_text()
            && !t.is_empty()
        {
            return Some(Content::Text(t));
        }
        let img = clip.get_image().ok()?;
        Some(Content::Image {
            width: img.width as u32,
            height: img.height as u32,
            rgba: img.bytes.into_owned(),
        })
    })
    .await
    .ok()
    .flatten()
}

#[cfg(target_os = "macos")]
mod pasteboard {
    use objc2_app_kit::NSPasteboard;

    /// Markers set by password managers (nspasteboard.org).
    const PRIVATE_TYPES: [&str; 2] = [
        "org.nspasteboard.ConcealedType",
        "org.nspasteboard.TransientType",
    ];

    pub fn change_count() -> isize {
        NSPasteboard::generalPasteboard().changeCount()
    }

    pub fn concealed() -> bool {
        let Some(types) = NSPasteboard::generalPasteboard().types() else {
            return false;
        };
        types
            .iter()
            .any(|t| PRIVATE_TYPES.contains(&t.to_string().as_str()))
    }
}

#[cfg(not(target_os = "macos"))]
mod pasteboard {
    use std::sync::atomic::{AtomicIsize, Ordering};

    /// No cheap change counter: report a change on every poll, the content key dedups.
    pub fn change_count() -> isize {
        static N: AtomicIsize = AtomicIsize::new(0);
        N.fetch_add(1, Ordering::Relaxed)
    }

    pub fn concealed() -> bool {
        false
    }
}
