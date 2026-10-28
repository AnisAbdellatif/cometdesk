//! The clipboard thread: owns the OS clipboard, applies items from the GUI and reports local
//! changes. On X11 and Wayland the owner of a clipboard selection must stay alive to serve
//! it, so one long-lived thread keeps the `arboard::Clipboard`.
//!
//! Change detection is polling. Windows has a cheap sequence number; elsewhere text is read
//! every tick, and images (expensive to read) only every few ticks when there is no text.

use std::sync::mpsc;
use std::time::Duration;

use arboard::{Clipboard, ImageData};
use glkvm_clip::{Content, Dedup};

use crate::log;

const TICK: Duration = Duration::from_millis(500);
const IMAGE_EVERY: u32 = 4;

pub enum ClipCommand {
    Set(Content),
}

/// New content on the local clipboard.
pub struct Local(pub Content);

pub fn spawn(
    cmds: mpsc::Receiver<ClipCommand>,
    local: mpsc::Sender<Local>,
) -> Result<(), arboard::Error> {
    // Open once here so a missing clipboard (no display) fails at start-up, not silently.
    let clip = Clipboard::new()?;
    std::thread::Builder::new()
        .name("clipboard".into())
        .spawn(move || run(clip, cmds, local))
        .expect("spawn clipboard thread");
    Ok(())
}

fn run(mut clip: Clipboard, cmds: mpsc::Receiver<ClipCommand>, local: mpsc::Sender<Local>) {
    let mut dedup = Dedup::default();
    let mut seq = Sequence::default();
    let mut tick = 0u32;
    // Whatever is on the clipboard at start-up counts as already seen.
    if let Some(c) = read(&mut clip, true) {
        dedup.observe_local(c.key());
    }
    loop {
        match cmds.recv_timeout(TICK) {
            Ok(ClipCommand::Set(content)) => {
                dedup.applied(content.key());
                if let Err(e) = write(&mut clip, content) {
                    log!("cannot set the clipboard: {e}");
                }
                seq.mark_seen();
                continue;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        if !seq.changed() {
            continue;
        }
        tick = tick.wrapping_add(1);
        if let Some(content) = read(&mut clip, tick.is_multiple_of(IMAGE_EVERY))
            && dedup.observe_local(content.key())
            && local.send(Local(content)).is_err()
        {
            return;
        }
    }
}

fn read(clip: &mut Clipboard, with_image: bool) -> Option<Content> {
    match clip.get_text() {
        Ok(t) if !t.is_empty() => return Some(Content::Text(t)),
        _ => {}
    }
    if !with_image && !cfg!(windows) {
        return None;
    }
    let img = clip.get_image().ok()?;
    Some(Content::Image {
        width: img.width as u32,
        height: img.height as u32,
        rgba: img.bytes.into_owned(),
    })
}

fn write(clip: &mut Clipboard, content: Content) -> Result<(), arboard::Error> {
    match content {
        Content::Text(t) => clip.set_text(t),
        Content::Image {
            width,
            height,
            rgba,
        } => clip.set_image(ImageData {
            width: width as usize,
            height: height as usize,
            bytes: rgba.into(),
        }),
    }
}

/// Windows: `GetClipboardSequenceNumber` tells whether anything changed since the last read.
/// Elsewhere: always "maybe".
#[derive(Default)]
struct Sequence {
    #[cfg(windows)]
    last: u32,
}

impl Sequence {
    #[cfg(windows)]
    fn changed(&mut self) -> bool {
        let now = unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() };
        let changed = now != self.last;
        self.last = now;
        changed
    }

    #[cfg(not(windows))]
    fn changed(&mut self) -> bool {
        true
    }

    /// After we set the clipboard ourselves.
    fn mark_seen(&mut self) {
        #[cfg(windows)]
        {
            self.last =
                unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() };
        }
    }
}
