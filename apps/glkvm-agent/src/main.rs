//! glkvm-agent: shares the target's clipboard with glkvm-gui through the Comet relay.
//!
//! Runs in the user's desktop session on the target (no admin rights). Connects to the relay
//! over the KVM's USB network link (`192.168.5.1:47474` by default) and speaks the
//! `glkvm-clip` protocol: an end-to-end encrypted session keyed by the pairing secret, so the
//! Comet only forwards ciphertext. The agent sends its clipboard only when the GUI allows it.
//!
//! ```text
//! glkvm-agent pair <code>     store the pairing code shown by glkvm-gui
//! glkvm-agent notify <title> [body]   show a notification on the Mac (for scripts)
//! glkvm-agent [run] [--relay HOST:PORT] [--log FILE]
//! glkvm-agent unpair          forget the pairing
//! glkvm-agent uninstall       forget the pairing and remove autostart
//! ```
//!
//! Normally installed by the installer the Comet serves on the USB link (bridge/target/),
//! which glkvm-gui types on the target for you. Only one instance runs per user.

mod capture;
mod clipboard;
mod config;
mod filesend;
mod info;
mod insert;
mod notify;
mod power;

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use glkvm_clip::{Content, Endpoint, Error, Event, FileReceiver, Hello, Policy, Role, Secret};

use clipboard::{ClipCommand, Local};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_RELAY: &str = "192.168.5.1:47474";
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(10);
/// After a pairing mismatch, retrying fast only fills the log.
const MISMATCH_BACKOFF: Duration = Duration::from_secs(30);
/// The GUI pings every 15 s once the session is up.
const IDLE_TIMEOUT: Duration = Duration::from_secs(45);
const READ_SLICE: Duration = Duration::from_millis(100);
/// Notify the user when an unplugged target drops to this percentage.
const BATTERY_LOW: u8 = 15;

/// Local port held while `run` is active, so a second instance (autostart plus a manual
/// start) refuses to run instead of fighting over the clipboard.
const INSTANCE_PORT: u16 = 47476;

/// `--log FILE`: append log lines there instead of stderr.
static LOG_FILE: std::sync::OnceLock<std::sync::Mutex<std::fs::File>> = std::sync::OnceLock::new();

macro_rules! log {
    ($($t:tt)*) => {
        $crate::log_line(&format!($($t)*))
    };
}
pub(crate) use log;

pub(crate) fn log_line(msg: &str) {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() % 86400)
        .unwrap_or(0);
    let line = format!("{:02}:{:02}:{:02} {msg}", t / 3600, t / 60 % 60, t % 60);
    match LOG_FILE.get() {
        Some(f) => {
            if let Ok(mut f) = f.lock() {
                let _ = writeln!(f, "{line}");
            }
        }
        None => eprintln!("{line}"),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["pair", code @ ..] if !code.is_empty() => pair(&code.join("")),
        ["unpair"] => match config::remove_secret() {
            Ok(()) => {
                println!("Pairing removed.");
                ExitCode::SUCCESS
            }
            Err(e) => fail(&format!("cannot remove the pairing: {e}")),
        },
        ["--version" | "-V"] => {
            println!("glkvm-agent {VERSION}");
            ExitCode::SUCCESS
        }
        ["uninstall"] => uninstall(),
        ["notify", title, body @ ..] => match notify::send(title, &body.join(" ")) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(&format!("cannot notify: {e}")),
        },
        ["run", opts @ ..] | opts => match run_options(opts) {
            Some((relay, log)) => run(relay, log),
            None => {
                eprintln!(
                    "usage: glkvm-agent pair <code> | glkvm-agent [run] [--relay HOST:PORT] [--log FILE]\n       \
                     glkvm-agent unpair | glkvm-agent uninstall | glkvm-agent notify <title> [body]"
                );
                ExitCode::from(2)
            }
        },
    }
}

fn run_options<'a>(mut opts: &[&'a str]) -> Option<(&'a str, Option<&'a str>)> {
    let (mut relay, mut log) = (DEFAULT_RELAY, None);
    loop {
        match opts {
            [] => return Some((relay, log)),
            ["--relay", r, rest @ ..] => (relay, opts) = (r, rest),
            ["--log", f, rest @ ..] => (log, opts) = (Some(*f), rest),
            _ => return None,
        }
    }
}

fn uninstall() -> ExitCode {
    if let Err(e) = config::remove_secret() {
        return fail(&format!("cannot remove the pairing: {e}"));
    }
    match config::remove_autostart() {
        Ok(()) => {
            println!("Pairing and autostart removed. A running agent stops at the next logon.");
            if let Ok(exe) = std::env::current_exe() {
                println!("Delete the program itself with: rm {}", exe.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("cannot remove autostart: {e}")),
    }
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("glkvm-agent: {msg}");
    ExitCode::FAILURE
}

fn pair(code: &str) -> ExitCode {
    let Some(secret) = Secret::from_code(code) else {
        return fail("that is not a valid pairing code (52 letters and digits 2-7)");
    };
    match config::save_secret(&secret) {
        Ok(path) => {
            println!("Paired. Secret stored in {}", path.display());
            println!("Start the agent with: glkvm-agent run");
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("cannot store the pairing: {e}")),
    }
}

fn run(relay: &str, log_file: Option<&str>) -> ExitCode {
    if let Some(path) = log_file {
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            Ok(f) => {
                let _ = LOG_FILE.set(std::sync::Mutex::new(f));
            }
            Err(e) => return fail(&format!("cannot open the log file {path}: {e}")),
        }
        detach_console();
    }
    let (note_tx, note_rx) = mpsc::channel();
    match std::net::TcpListener::bind(("127.0.0.1", INSTANCE_PORT)) {
        // Held for as long as the process lives, by the thread that answers `notify`.
        Ok(l) => notify::serve(l, note_tx),
        Err(_) => {
            log!("another glkvm-agent is already running for this user; exiting");
            return ExitCode::SUCCESS;
        }
    };
    let secret = match config::load_secret() {
        Ok(Some(s)) => s,
        Ok(None) => return fail("not paired: run `glkvm-agent pair <code>` first"),
        Err(e) => return fail(&format!("cannot read the pairing: {e}")),
    };
    let addr = match relay.to_socket_addrs().ok().and_then(|mut a| a.next()) {
        Some(a) => a,
        None => return fail(&format!("bad relay address {relay}")),
    };
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (local_tx, local_rx) = mpsc::channel();
    if let Err(e) = clipboard::spawn(cmd_rx, local_tx) {
        return fail(&format!("cannot open the clipboard: {e}"));
    }
    log!("glkvm-agent {VERSION}: relay {addr}");
    let hello = Hello::local(VERSION, config::host_name());
    let mut backoff = BACKOFF_MIN;
    loop {
        let wait = match session(addr, &secret, &hello, &cmd_tx, &local_rx, &note_rx) {
            Ok(()) => BACKOFF_MIN,
            Err(SessionEnd::Mismatch) => {
                log!("pairing mismatch: the GUI uses a different code; re-pair this agent");
                MISMATCH_BACKOFF
            }
            Err(SessionEnd::Lost(reason, was_up)) => {
                log!("connection: {reason}");
                if was_up {
                    backoff = BACKOFF_MIN;
                }
                let w = backoff;
                backoff = (backoff * 2).min(BACKOFF_MAX);
                w
            }
            Err(SessionEnd::ClipboardGone) => return fail("clipboard thread stopped"),
        };
        std::thread::sleep(wait);
    }
}

/// Started hidden with a log file (Windows autostart): drop the console window.
fn detach_console() {
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Console::FreeConsole();
    }
}

enum SessionEnd {
    Mismatch,
    /// Reason, and whether a session had been established (resets the backoff).
    Lost(String, bool),
    ClipboardGone,
}

fn session(
    addr: SocketAddr,
    secret: &Secret,
    hello: &Hello,
    cmd_tx: &mpsc::Sender<ClipCommand>,
    local_rx: &mpsc::Receiver<Local>,
    note_rx: &mpsc::Receiver<notify::Note>,
) -> Result<(), SessionEnd> {
    let lost = |e: std::io::Error, up| SessionEnd::Lost(e.to_string(), up);
    let mut stream =
        TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(|e| lost(e, false))?;
    stream.set_nodelay(true).ok();
    stream
        .set_read_timeout(Some(READ_SLICE))
        .map_err(|e| lost(e, false))?;
    let mut ep = Endpoint::new(Role::Agent, secret, hello.clone())
        .map_err(|e| SessionEnd::Lost(e.to_string(), false))?;
    let mut policy = Policy {
        agent_may_send: false,
    };
    let mut last_rx = Instant::now();
    let mut probe = info::Probe::new();
    let mut files = FileReceiver::new(std::env::temp_dir().join("glkvm-clip"));
    let mut outbox = filesend::Outbox::default();
    let mut fast_read = false;
    let mut battery_low = false;
    let mut buf = vec![0u8; 64 * 1024];
    // Changes seen before the session was up are not sent: only what changes while connected.
    while local_rx.try_recv().is_ok() {}
    loop {
        let up = ep.is_ready();
        match stream.read(&mut buf) {
            Ok(0) => return Err(SessionEnd::Lost("relay closed the connection".into(), up)),
            Ok(n) => {
                last_rx = Instant::now();
                let events = ep.receive(&buf[..n]).map_err(|e| match e {
                    Error::Handshake => SessionEnd::Mismatch,
                    e => SessionEnd::Lost(e.to_string(), up),
                })?;
                for ev in events {
                    match ev {
                        Event::Ready(peer) => {
                            log!("connected to {} ({} {})", peer.name, peer.os, peer.version)
                        }
                        Event::Policy(p) => {
                            if p != policy {
                                log!(
                                    "policy: {}",
                                    if p.agent_may_send {
                                        "sharing this clipboard"
                                    } else {
                                        "receiving only"
                                    }
                                );
                            }
                            policy = p;
                        }
                        Event::Insert { id, text, paste } => {
                            let reply = insert::insert(&text, paste, cmd_tx);
                            log!("insert of {} chars: {reply:?}", text.chars().count());
                            ep.send_insert_reply(id, reply)
                                .map_err(|e| SessionEnd::Lost(e.to_string(), true))?;
                        }
                        Event::InsertPasted { .. } => {
                            cmd_tx
                                .send(ClipCommand::RestoreSoon)
                                .map_err(|_| SessionEnd::ClipboardGone)?;
                        }
                        Event::Capture { id, display } => {
                            let sent = match capture::screenshot(display) {
                                Ok(png) => {
                                    log!("screenshot for the GUI: {} bytes", png.len());
                                    ep.send_screenshot(id, &png)
                                }
                                Err(reason) => {
                                    log!("screenshot failed: {reason}");
                                    ep.send_capture_failed(id, reason)
                                }
                            };
                            sent.map_err(|e| SessionEnd::Lost(e.to_string(), true))?;
                        }
                        Event::Power { id, action } => {
                            let result = power::run(action);
                            log!("power {action:?}: {result:?}");
                            ep.send_ack(id, result)
                                .map_err(|e| SessionEnd::Lost(e.to_string(), true))?;
                        }
                        Event::FileOffer {
                            id,
                            name,
                            size,
                            purpose,
                        } => {
                            log!("file offered: {size} bytes ({purpose:?})");
                            if let Err(e) = files.offer(id, &name, size, purpose) {
                                log!("file refused: {e}");
                                ep.send_file_result(id, Err(e))
                                    .map_err(|e| SessionEnd::Lost(e.to_string(), true))?;
                            }
                        }
                        Event::FileData { id, data } => {
                            if let Err(e) = files.data(id, &data) {
                                log!("file failed: {e}");
                                ep.send_file_result(id, Err(e))
                                    .map_err(|e| SessionEnd::Lost(e.to_string(), true))?;
                            }
                        }
                        Event::FileResult { id, result } => outbox.result(&mut ep, id, result),
                        Event::FileEnd { id, intact } => {
                            let result = files.end(id, intact);
                            log!("file done: {result:?}");
                            ep.send_file_result(id, result)
                                .map_err(|e| SessionEnd::Lost(e.to_string(), true))?;
                        }
                        Event::FileCancel { id } => files.cancel(id),
                        Event::FilesReady { ids } => match files.take_staged(&ids) {
                            Some(paths) => {
                                log!("copied files from the GUI: {}", paths.len());
                                cmd_tx
                                    .send(ClipCommand::Set(Content::Files(paths)))
                                    .map_err(|_| SessionEnd::ClipboardGone)?;
                            }
                            None => log!("copied files from the GUI are incomplete"),
                        },
                        Event::SetLayout { id, layout } => {
                            let result = info::set_layout(&layout);
                            log!("layout {layout}: {result:?}");
                            probe.refresh();
                            ep.send_ack(id, result)
                                .map_err(|e| SessionEnd::Lost(e.to_string(), true))?;
                        }
                        Event::Notify { .. }
                        | Event::InsertReply { .. }
                        | Event::Ack { .. }
                        | Event::Info(_)
                        | Event::Screenshot { .. }
                        | Event::CaptureFailed { .. } => {}
                        Event::Clip(content) => {
                            log!("received {content:?}");
                            cmd_tx
                                .send(ClipCommand::Set(content))
                                .map_err(|_| SessionEnd::ClipboardGone)?;
                        }
                    }
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => return Err(lost(e, up)),
        }
        if ep.is_ready() && last_rx.elapsed() > IDLE_TIMEOUT {
            return Err(SessionEnd::Lost("no traffic from the GUI".into(), true));
        }
        loop {
            match local_rx.try_recv() {
                Ok(Local(Content::Files(paths))) if ep.is_ready() && policy.agent_may_send => {
                    log!("sending {} copied files", paths.len());
                    outbox.start(&mut ep, paths);
                }
                Ok(Local(content)) if ep.is_ready() && policy.agent_may_send => {
                    log!("sending {content:?}");
                    if let Err(e) = ep.send_clip(&content) {
                        log!("cannot send: {e}");
                    }
                }
                Ok(_) => {}
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Err(SessionEnd::ClipboardGone),
            }
        }
        if ep.is_ready() && !ep.has_outgoing() {
            outbox.pump(&mut ep);
        }
        // While streaming a file there is no time to wait for the next message.
        if outbox.busy() != fast_read {
            fast_read = outbox.busy();
            let slice = if fast_read {
                Duration::from_millis(1)
            } else {
                READ_SLICE
            };
            stream.set_read_timeout(Some(slice)).ok();
        }
        while let Ok(note) = note_rx.try_recv() {
            if ep.is_ready() {
                ep.send_notify(&note.title, &note.body)
                    .map_err(|e| lost(std::io::Error::other(e), true))?;
            } else {
                notify::dropped(&note);
            }
        }
        if ep.is_ready()
            && let Some(i) = probe.changed()
        {
            let low = i
                .battery
                .is_some_and(|b| b.percent <= BATTERY_LOW && !b.charging);
            if low && !battery_low {
                let pct = i.battery.map_or(0, |b| b.percent);
                ep.send_notify("Battery low", &format!("The target is at {pct}%"))
                    .map_err(|e| lost(std::io::Error::other(e), true))?;
            }
            battery_low = low;
            log!("target info: {i:?}");
            ep.send_info(&i)
                .map_err(|e| lost(std::io::Error::other(e), true))?;
        }
        if ep.has_outgoing() {
            stream
                .write_all(&ep.take_outgoing())
                .map_err(|e| lost(e, ep.is_ready()))?;
        }
    }
}
