//! cometdesk-agent: shares the target's clipboard with cometdesk-gui through the Comet relay.
//!
//! Runs in the user's desktop session on the target (no admin rights). Connects to the relay
//! over the KVM's USB network link (`192.168.5.1:47474` by default) and speaks the
//! `cometdesk-clip` protocol: an end-to-end encrypted session keyed by the pairing secret, so the
//! Comet only forwards ciphertext. The agent sends its clipboard only when the GUI allows it.
//!
//! ```text
//! cometdesk-agent pair <code>     store the pairing code shown by cometdesk-gui
//! cometdesk-agent notify <title> [body]   show a notification on the Mac (for scripts)
//! cometdesk-agent [run] [--relay HOST:PORT] [--log FILE]
//! cometdesk-agent unpair          forget the pairing
//! cometdesk-agent uninstall       forget the pairing and remove autostart
//! ```
//!
//! Normally installed by the installer the Comet serves on the USB link (bridge/target/),
//! which cometdesk-gui types on the target for you. Only one instance runs per user.

mod browse;
mod capture;
mod clipboard;
mod config;
mod display;
mod filesend;
mod info;
mod insert;
mod notify;
mod power;
mod session;
mod update;
mod volume;

use std::io::Write;
use std::net::ToSocketAddrs;
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use cometdesk_clip::{Hello, Secret};

use session::{SessionEnd, session};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_RELAY: &str = "192.168.5.1:47474";
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(10);
/// After a pairing mismatch, retrying fast only fills the log.
const MISMATCH_BACKOFF: Duration = Duration::from_secs(30);

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
            println!("cometdesk-agent {VERSION}");
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
                    "usage: cometdesk-agent pair <code> | cometdesk-agent [run] [--relay HOST:PORT] [--log FILE]\n       \
                     cometdesk-agent unpair | cometdesk-agent uninstall | cometdesk-agent notify <title> [body]"
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
    eprintln!("cometdesk-agent: {msg}");
    ExitCode::FAILURE
}

fn pair(code: &str) -> ExitCode {
    let Some(secret) = Secret::from_code(code) else {
        return fail("that is not a valid pairing code (52 letters and digits 2-7)");
    };
    match config::save_secret(&secret) {
        Ok(path) => {
            println!("Paired. Secret stored in {}", path.display());
            println!("Start the agent with: cometdesk-agent run");
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
    update::cleanup();
    let (note_tx, note_rx) = mpsc::channel();
    // After an update the predecessor is still closing down.
    let tries = if std::env::var_os(update::RESTARTED).is_some() {
        50
    } else {
        1
    };
    match notify::bind(tries) {
        // Held for as long as the process lives, by the thread that answers `notify`.
        Ok(guard) => notify::serve(guard, note_tx),
        Err(notify::BindError::AlreadyRunning) => {
            log!("another cometdesk-agent is already running for this user; exiting");
            return ExitCode::SUCCESS;
        }
        Err(notify::BindError::Failed(e)) => {
            return fail(&format!("cannot start instance listener: {e}"));
        }
    }
    let secret = match config::load_secret() {
        Ok(Some(s)) => s,
        Ok(None) => return fail("not paired: run `cometdesk-agent pair <code>` first"),
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
    log!("cometdesk-agent {VERSION}: relay {addr}");
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
