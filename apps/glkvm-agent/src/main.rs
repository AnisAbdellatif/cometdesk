//! glkvm-agent: shares the target's clipboard with glkvm-gui through the Comet relay.
//!
//! Runs in the user's desktop session on the target (no admin rights). Connects to the relay
//! over the KVM's USB network link (`192.168.5.1:47474` by default) and speaks the
//! `glkvm-clip` protocol: an end-to-end encrypted session keyed by the pairing secret, so the
//! Comet only forwards ciphertext. The agent sends its clipboard only when the GUI allows it.
//!
//! ```text
//! glkvm-agent pair <code>     store the pairing code shown by glkvm-gui
//! glkvm-agent [run] [--relay HOST:PORT]
//! glkvm-agent unpair
//! ```

mod clipboard;
mod config;

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use glkvm_clip::{Endpoint, Error, Event, Hello, Policy, Role, Secret};

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

macro_rules! log {
    ($($t:tt)*) => {{
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() % 86400)
            .unwrap_or(0);
        eprintln!("{:02}:{:02}:{:02} {}", t / 3600, t / 60 % 60, t % 60, format!($($t)*));
    }};
}
pub(crate) use log;

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
        [] | ["run"] => run(DEFAULT_RELAY),
        ["run", "--relay", relay] | ["--relay", relay] => run(relay),
        _ => {
            eprintln!(
                "usage: glkvm-agent pair <code> | glkvm-agent [run] [--relay HOST:PORT] | glkvm-agent unpair"
            );
            ExitCode::from(2)
        }
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

fn run(relay: &str) -> ExitCode {
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
        let wait = match session(addr, &secret, &hello, &cmd_tx, &local_rx) {
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
        if ep.has_outgoing() {
            stream
                .write_all(&ep.take_outgoing())
                .map_err(|e| lost(e, ep.is_ready()))?;
        }
    }
}
