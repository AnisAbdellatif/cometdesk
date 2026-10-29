//! The clipboard thread: owns the OS clipboard, applies items from the GUI and reports local
//! changes. On X11 and Wayland the owner of a clipboard selection must stay alive to serve
//! it, so one long-lived thread keeps the `arboard::Clipboard`.
//!
//! Change detection is polling. Windows has a cheap sequence number; elsewhere text is read
//! every tick, and images (expensive to read) only every few ticks when there is no text.
//!
//! Text insertion (`crate::insert`) borrows the clipboard: the text goes on it for one paste
//! keystroke, then the previous content comes back, unless the user copied something else
//! in between.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use arboard::{Clipboard, ImageData};
use cometdesk_clip::{Content, ContentKey, Dedup};

use crate::log;

const TICK: Duration = Duration::from_millis(500);
const IMAGE_EVERY: u32 = 4;

/// After the paste keystroke: time for the focused app to read the clipboard.
const RESTORE_AFTER_PASTE: Duration = Duration::from_millis(1000);
/// If the GUI never confirms the paste, restore anyway.
const RESTORE_AT_LATEST: Duration = Duration::from_secs(10);

pub enum ClipCommand {
    Set(Content),
    /// Put text on the clipboard for one paste; answer when it is there.
    SetForInsert {
        text: String,
        done: mpsc::Sender<Result<(), String>>,
    },
    /// The paste keystroke was sent: restore the previous clipboard soon.
    RestoreSoon,
}

struct Restore {
    previous: Option<Content>,
    inserted: ContentKey,
    due: Instant,
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
    let mut restore: Option<Restore> = None;
    loop {
        let timeout = restore.as_ref().map_or(TICK, |r| {
            r.due.saturating_duration_since(Instant::now()).min(TICK)
        });
        match cmds.recv_timeout(timeout) {
            Ok(ClipCommand::Set(content)) => {
                dedup.applied(content.key());
                if let Err(e) = write(&mut clip, content) {
                    log!("cannot set the clipboard: {e}");
                }
                seq.mark_seen();
                continue;
            }
            Ok(ClipCommand::SetForInsert { text, done }) => {
                let content = Content::Text(text);
                let inserted = content.key();
                // Keep the oldest saved content if inserts follow each other quickly.
                let previous = match restore.take() {
                    Some(r) => r.previous,
                    None => read(&mut clip, true),
                };
                dedup.applied(inserted);
                let r = write(&mut clip, content).map_err(|e| e.to_string());
                seq.mark_seen();
                restore = Some(Restore {
                    previous,
                    inserted,
                    due: Instant::now() + RESTORE_AT_LATEST,
                });
                let _ = done.send(r);
                continue;
            }
            Ok(ClipCommand::RestoreSoon) => {
                if let Some(r) = restore.as_mut() {
                    r.due = Instant::now() + RESTORE_AFTER_PASTE;
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        if restore.as_ref().is_some_and(|r| Instant::now() >= r.due) {
            let r = restore.take().unwrap();
            // Only if the inserted text is still there: the user may have copied since.
            if read(&mut clip, false).map(|c| c.key()) == Some(r.inserted)
                && let Some(prev) = r.previous
            {
                dedup.applied(prev.key());
                if let Err(e) = write(&mut clip, prev) {
                    log!("cannot restore the clipboard: {e}");
                }
                seq.mark_seen();
            }
            continue;
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
    if let Ok(files) = clip.get().file_list()
        && !files.is_empty()
    {
        return Some(Content::Files(files));
    }
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
        Content::Files(paths) => clip.set().file_list(&paths),
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
