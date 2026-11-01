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
    let mut sensitive_checker = SensitiveChecker::new();
    let mut tick = 0u32;
    // Whatever is on the clipboard at start-up counts as already seen.
    if !sensitive_checker.is_sensitive()
        && let Some(c) = read(&mut clip, true)
    {
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
                    None => {
                        if sensitive_checker.is_sensitive() {
                            None
                        } else {
                            read(&mut clip, true)
                        }
                    }
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
        if sensitive_checker.is_sensitive() {
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

/// Detects sensitive password manager markers on the clipboard.
#[cfg(windows)]
struct SensitiveChecker {
    fmt_ignore: u32,
    fmt_history: u32,
    fmt_exclude: u32,
}

#[cfg(windows)]
impl SensitiveChecker {
    fn new() -> Self {
        use windows_sys::Win32::System::DataExchange::*;
        unsafe {
            Self {
                fmt_ignore: RegisterClipboardFormatA(b"Clipboard Viewer Ignore\0".as_ptr()),
                fmt_history: RegisterClipboardFormatA(b"CanIncludeInClipboardHistory\0".as_ptr()),
                fmt_exclude: RegisterClipboardFormatA(
                    b"ExcludeClipboardContentFromMonitorProcessing\0".as_ptr(),
                ),
            }
        }
    }

    fn is_sensitive(&mut self) -> bool {
        use windows_sys::Win32::System::DataExchange::*;
        unsafe {
            if self.fmt_ignore != 0 && IsClipboardFormatAvailable(self.fmt_ignore) != 0 {
                return true;
            }
            if self.fmt_history != 0 && IsClipboardFormatAvailable(self.fmt_history) != 0 {
                return true;
            }
            if self.fmt_exclude != 0 && IsClipboardFormatAvailable(self.fmt_exclude) != 0 {
                return true;
            }
            false
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
struct SensitiveChecker {
    conn: Option<x11rb::rust_connection::RustConnection>,
    atom_hint: Option<x11rb::protocol::xproto::Atom>,
    atom_clipboard: x11rb::protocol::xproto::Atom,
    atom_targets: x11rb::protocol::xproto::Atom,
    atom_prop: x11rb::protocol::xproto::Atom,
    window: x11rb::protocol::xproto::Window,
}

#[cfg(all(unix, not(target_os = "macos")))]
impl SensitiveChecker {
    fn new() -> Self {
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::*;

        let Ok((conn, screen_num)) = x11rb::connect(None) else {
            return Self {
                conn: None,
                atom_hint: None,
                atom_clipboard: 0,
                atom_targets: 0,
                atom_prop: 0,
                window: 0,
            };
        };

        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        let atom_clipboard = conn
            .intern_atom(false, b"CLIPBOARD")
            .ok()
            .and_then(|c| c.reply().ok())
            .map_or(0, |r| r.atom);
        let atom_targets = conn
            .intern_atom(false, b"TARGETS")
            .ok()
            .and_then(|c| c.reply().ok())
            .map_or(0, |r| r.atom);
        let atom_prop = conn
            .intern_atom(false, b"_COMETDESK_CLIPBOARD_TARGETS")
            .ok()
            .and_then(|c| c.reply().ok())
            .map_or(0, |r| r.atom);

        let window = if let Ok(win) = conn.generate_id() {
            if conn
                .create_window(
                    screen.root_depth,
                    win,
                    root,
                    0,
                    0,
                    1,
                    1,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    screen.root_visual,
                    &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
                )
                .is_ok()
            {
                let _ = conn.flush();
                win
            } else {
                0
            }
        } else {
            0
        };

        Self {
            conn: Some(conn),
            atom_hint: None,
            atom_clipboard,
            atom_targets,
            atom_prop,
            window,
        }
    }

    fn is_sensitive(&mut self) -> bool {
        use x11rb::connection::Connection;
        use x11rb::protocol::Event;
        use x11rb::protocol::xproto::*;

        let Some(ref conn) = self.conn else {
            return false;
        };
        if self.window == 0 || self.atom_clipboard == 0 || self.atom_targets == 0 {
            return false;
        }

        // Fast check: has "x-kde-passwordManagerHint" been interned on the X server?
        if self.atom_hint.is_none() {
            if let Ok(reply) = conn
                .intern_atom(true, b"x-kde-passwordManagerHint")
                .map_err(x11rb::errors::ReplyError::from)
                .and_then(|c| c.reply())
            {
                if reply.atom != x11rb::NONE {
                    self.atom_hint = Some(reply.atom);
                } else {
                    return false;
                }
            } else {
                return false;
            }
        }
        let hint_atom = match self.atom_hint {
            Some(a) if a != x11rb::NONE => a,
            _ => return false,
        };

        // Check if CLIPBOARD has an owner.
        let Ok(owner_reply) = conn
            .get_selection_owner(self.atom_clipboard)
            .map_err(x11rb::errors::ReplyError::from)
            .and_then(|c| c.reply())
        else {
            return false;
        };
        if owner_reply.owner == x11rb::NONE {
            return false;
        }

        // Request TARGETS from the owner into our property.
        let _ = conn.convert_selection(
            self.window,
            self.atom_clipboard,
            self.atom_targets,
            self.atom_prop,
            x11rb::CURRENT_TIME,
        );
        let _ = conn.flush();

        let mut sensitive = false;
        let deadline = Instant::now() + Duration::from_millis(100);
        while Instant::now() < deadline {
            let Ok(ev) = conn.poll_for_event() else {
                break;
            };
            if let Some(Event::SelectionNotify(sn)) = ev {
                if sn.property != x11rb::NONE {
                    if let Ok(prop) = conn
                        .get_property(false, self.window, sn.property, AtomEnum::ATOM, 0, 1024)
                        .map_err(x11rb::errors::ReplyError::from)
                        .and_then(|c| c.reply())
                    {
                        if let Some(atoms) = prop.value32() {
                            for atom in atoms {
                                if atom == hint_atom {
                                    sensitive = true;
                                    break;
                                }
                            }
                        }
                    }
                    let _ = conn.delete_property(self.window, sn.property);
                    let _ = conn.flush();
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        sensitive
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
impl Drop for SensitiveChecker {
    fn drop(&mut self) {
        if let Some(ref conn) = self.conn {
            use x11rb::connection::Connection;
            use x11rb::protocol::xproto::ConnectionExt;
            if self.window != 0 {
                let _ = conn.destroy_window(self.window);
                let _ = conn.flush();
            }
        }
    }
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
struct SensitiveChecker;

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
impl SensitiveChecker {
    fn new() -> Self {
        Self
    }
    fn is_sensitive(&mut self) -> bool {
        false
    }
}
