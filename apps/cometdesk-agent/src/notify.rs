//! Lets scripts on the target raise a notification on the user's Mac:
//! `cometdesk-agent notify "Build finished" "took 4 min"`. The command talks to the running
//! agent over a local IPC channel (Unix Domain Socket on Unix, authenticated local socket
//! on Windows) that also serves as the single-instance lock.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

use crate::log;

/// Longest request line we read; anything longer is cut by the protocol anyway.
const MAX_REQUEST: u64 = 4096;
const IO_TIMEOUT: Duration = Duration::from_secs(2);
/// Rate limit so a runaway script cannot flood the Mac.
const BURST: u32 = 5;
const REFILL: Duration = Duration::from_secs(2);

#[cfg(windows)]
const INSTANCE_PORT: u16 = 47476;

pub struct Note {
    pub title: String,
    pub body: String,
}

#[derive(Debug)]
pub enum BindError {
    AlreadyRunning,
    Failed(String),
}

#[cfg(unix)]
pub struct InstanceGuard {
    listener: UnixListener,
    path: PathBuf,
}

#[cfg(unix)]
impl Drop for InstanceGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(windows)]
pub struct InstanceGuard {
    listener: std::net::TcpListener,
    token_path: PathBuf,
    token: String,
}

#[cfg(windows)]
impl Drop for InstanceGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.token_path);
    }
}

#[cfg(unix)]
pub fn socket_path() -> PathBuf {
    let candidate =
        if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
            PathBuf::from(runtime).join("cometdesk-agent.sock")
        } else if let Some(state) = std::env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty()) {
            PathBuf::from(state)
                .join("cometdesk-agent")
                .join("cometdesk-agent.sock")
        } else if let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
            PathBuf::from(home).join(".local/state/cometdesk-agent/cometdesk-agent.sock")
        } else {
            PathBuf::new()
        };
    if !candidate.as_os_str().is_empty() && candidate.as_os_str().len() < 100 {
        return candidate;
    }
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    let uid = unsafe { getuid() };
    PathBuf::from(format!("/tmp/cometdesk-agent-{uid}.sock"))
}

#[cfg(windows)]
pub fn token_path() -> PathBuf {
    if let Some(local) = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) {
        return PathBuf::from(local)
            .join("cometdesk-agent")
            .join("ipc.token");
    }
    if let Some(user) = std::env::var_os("USERPROFILE").filter(|v| !v.is_empty()) {
        return PathBuf::from(user)
            .join("AppData")
            .join("Local")
            .join("cometdesk-agent")
            .join("ipc.token");
    }
    std::env::temp_dir().join(format!("cometdesk-agent-{}.token", std::process::id()))
}

/// Acquires the single-instance lock and notification listener.
pub fn bind(tries: usize) -> Result<InstanceGuard, BindError> {
    #[cfg(unix)]
    {
        bind_to(socket_path(), tries)
    }
    #[cfg(windows)]
    {
        bind_windows(token_path(), tries)
    }
}

#[cfg(unix)]
pub fn bind_to(path: PathBuf, tries: usize) -> Result<InstanceGuard, BindError> {
    if let Some(parent) = path.parent() {
        cometdesk_clip::create_restricted_dir_all(parent)
            .map_err(|e| BindError::Failed(format!("cannot create directory: {e}")))?;
    }
    for i in 0..tries {
        if i > 0 {
            std::thread::sleep(Duration::from_millis(200));
        }
        match UnixListener::bind(&path) {
            Ok(listener) => {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
                return Ok(InstanceGuard { listener, path });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                // Check if another instance is actively responding.
                match UnixStream::connect(&path) {
                    Ok(_) => {
                        if i + 1 >= tries {
                            return Err(BindError::AlreadyRunning);
                        }
                    }
                    Err(_) => {
                        // Stale socket file from previous run or crash: remove and retry.
                        let _ = std::fs::remove_file(&path);
                        if let Ok(listener) = UnixListener::bind(&path) {
                            use std::os::unix::fs::PermissionsExt;
                            let _ = std::fs::set_permissions(
                                &path,
                                std::fs::Permissions::from_mode(0o600),
                            );
                            return Ok(InstanceGuard { listener, path });
                        }
                    }
                }
            }
            Err(e) => return Err(BindError::Failed(e.to_string())),
        }
    }
    Err(BindError::AlreadyRunning)
}

#[cfg(windows)]
fn bind_windows(token_path: PathBuf, tries: usize) -> Result<InstanceGuard, BindError> {
    if let Some(parent) = token_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    for i in 0..tries {
        if i > 0 {
            std::thread::sleep(Duration::from_millis(200));
        }
        if let Ok(content) = std::fs::read_to_string(&token_path) {
            let mut lines = content.lines();
            if let (Some(port_str), Some(tok)) = (lines.next(), lines.next()) {
                if let Ok(port) = port_str.trim().parse::<u16>() {
                    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
                    if let Ok(mut stream) =
                        std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(300))
                    {
                        let _ = stream.write_all(format!("ping\n{tok}\n").as_bytes());
                        let mut resp = String::new();
                        stream
                            .set_read_timeout(Some(Duration::from_millis(300)))
                            .ok();
                        if stream.read_to_string(&mut resp).is_ok() && resp.trim() == "ok" {
                            if i + 1 >= tries {
                                return Err(BindError::AlreadyRunning);
                            }
                            continue;
                        }
                    }
                }
            }
        }
        let _ = std::fs::remove_file(&token_path);
        let listener = std::net::TcpListener::bind(("127.0.0.1", INSTANCE_PORT))
            .or_else(|_| std::net::TcpListener::bind(("127.0.0.1", 0)))
            .map_err(|e| BindError::Failed(e.to_string()))?;
        let port = listener
            .local_addr()
            .map_err(|e| BindError::Failed(e.to_string()))?
            .port();
        let token = cometdesk_clip::Secret::generate().to_code();
        std::fs::write(&token_path, format!("{port}\n{token}\n"))
            .map_err(|e| BindError::Failed(e.to_string()))?;
        return Ok(InstanceGuard {
            listener,
            token_path,
            token,
        });
    }
    Err(BindError::AlreadyRunning)
}

/// Serves `notify` requests from a background thread.
pub fn serve(guard: InstanceGuard, tx: mpsc::Sender<Note>) {
    std::thread::Builder::new()
        .name("notify".into())
        .spawn(move || {
            #[cfg(unix)]
            serve_unix(guard, tx);
            #[cfg(windows)]
            serve_windows(guard, tx);
        })
        .expect("spawn notify thread");
}

#[cfg(unix)]
fn serve_unix(guard: InstanceGuard, tx: mpsc::Sender<Note>) {
    let (mut tokens, mut at) = (BURST, Instant::now());
    for conn in guard.listener.incoming().flatten() {
        let refill = (at.elapsed().as_secs_f64() / REFILL.as_secs_f64()) as u32;
        if refill > 0 {
            tokens = (tokens + refill).min(BURST);
            at = Instant::now();
        }
        let reply = match read_note_unix(&conn) {
            Some(_) if tokens == 0 => "error: too many notifications\n",
            Some(note) => {
                tokens -= 1;
                let _ = tx.send(note);
                "ok\n"
            }
            None => "error: bad request\n",
        };
        let _ = (&conn).write_all(reply.as_bytes());
    }
}

#[cfg(unix)]
fn read_note_unix(conn: &UnixStream) -> Option<Note> {
    conn.set_read_timeout(Some(IO_TIMEOUT)).ok()?;
    let mut text = String::new();
    conn.take(MAX_REQUEST).read_to_string(&mut text).ok()?;
    let mut parts = text.splitn(3, '\n');
    (parts.next()? == "notify").then_some(())?;
    let title = parts.next()?.trim().to_owned();
    let body = parts.next().unwrap_or("").trim().to_owned();
    (!title.is_empty()).then_some(Note { title, body })
}

#[cfg(windows)]
fn serve_windows(guard: InstanceGuard, tx: mpsc::Sender<Note>) {
    let (mut tokens, mut at) = (BURST, Instant::now());
    let token = guard.token;
    for conn in guard.listener.incoming().flatten() {
        conn.set_read_timeout(Some(IO_TIMEOUT)).ok();
        let refill = (at.elapsed().as_secs_f64() / REFILL.as_secs_f64()) as u32;
        if refill > 0 {
            tokens = (tokens + refill).min(BURST);
            at = Instant::now();
        }
        let mut text = String::new();
        let _ = (&conn).take(MAX_REQUEST).read_to_string(&mut text);
        let mut lines = text.splitn(4, '\n');
        let reply = match lines.next() {
            Some("ping") => {
                let tok = lines.next().unwrap_or("").trim();
                if tok == token {
                    "ok\n"
                } else {
                    "error: unauthorized\n"
                }
            }
            Some(tok) if tok.trim() == token => match lines.next() {
                Some("notify") => {
                    let title = lines.next().unwrap_or("").trim().to_owned();
                    let body = lines.next().unwrap_or("").trim().to_owned();
                    if title.is_empty() {
                        "error: bad request\n"
                    } else if tokens == 0 {
                        "error: too many notifications\n"
                    } else {
                        tokens -= 1;
                        let _ = tx.send(Note { title, body });
                        "ok\n"
                    }
                }
                _ => "error: bad request\n",
            },
            _ => "error: unauthorized\n",
        };
        let _ = (&conn).write_all(reply.as_bytes());
    }
}

/// The `notify` command: hands the message to the running agent.
pub fn send(title: &str, body: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        send_to_path(&socket_path(), title, body)
    }
    #[cfg(windows)]
    {
        send_windows(&token_path(), title, body)
    }
}

#[cfg(unix)]
pub fn send_to_path(path: &Path, title: &str, body: &str) -> Result<(), String> {
    let mut conn = UnixStream::connect(path).map_err(|_| "the agent is not running".to_string())?;
    conn.set_read_timeout(Some(IO_TIMEOUT)).ok();
    let title = title.replace('\n', " ");
    conn.write_all(format!("notify\n{title}\n{body}").as_bytes())
        .and_then(|()| conn.shutdown(std::net::Shutdown::Write))
        .map_err(|e| e.to_string())?;
    let mut reply = String::new();
    conn.read_to_string(&mut reply).map_err(|e| e.to_string())?;
    match reply.trim() {
        "ok" => Ok(()),
        other => Err(other.trim_start_matches("error: ").to_owned()),
    }
}

#[cfg(windows)]
pub fn send_windows(token_path: &Path, title: &str, body: &str) -> Result<(), String> {
    let content =
        std::fs::read_to_string(token_path).map_err(|_| "the agent is not running".to_string())?;
    let mut lines = content.lines();
    let port = lines
        .next()
        .ok_or("the agent is not running")?
        .trim()
        .parse::<u16>()
        .map_err(|_| "the agent is not running")?;
    let token = lines.next().ok_or("the agent is not running")?.trim();
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut conn = std::net::TcpStream::connect_timeout(&addr, IO_TIMEOUT)
        .map_err(|_| "the agent is not running".to_string())?;
    conn.set_read_timeout(Some(IO_TIMEOUT)).ok();
    let title = title.replace('\n', " ");
    conn.write_all(format!("{token}\nnotify\n{title}\n{body}").as_bytes())
        .and_then(|()| conn.shutdown(std::net::Shutdown::Write))
        .map_err(|e| e.to_string())?;
    let mut reply = String::new();
    conn.read_to_string(&mut reply).map_err(|e| e.to_string())?;
    match reply.trim() {
        "ok" => Ok(()),
        other => Err(other.trim_start_matches("error: ").to_owned()),
    }
}

/// The agent forwards only while the GUI is connected; say so in the log, not to the script.
pub fn dropped(note: &Note) {
    log!(
        "notification dropped, the GUI is not connected: {}",
        note.title
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn requests_reach_the_agent_and_floods_are_refused() {
        use std::os::unix::fs::PermissionsExt;
        let sock = std::path::PathBuf::from(format!("/tmp/cd-test-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&sock);
        let (tx, rx) = mpsc::channel();
        let guard = bind_to(sock.clone(), 1).expect("bind instance lock");
        let meta = std::fs::metadata(&sock).expect("socket file exists");
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);

        serve(guard, tx);
        send_to_path(&sock, "Build", "done\nin 4 min").unwrap();
        let note = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            (note.title.as_str(), note.body.as_str()),
            ("Build", "done\nin 4 min")
        );
        for _ in 1..BURST {
            send_to_path(&sock, "x", "").unwrap();
        }
        assert_eq!(
            send_to_path(&sock, "x", "").unwrap_err(),
            "too many notifications"
        );
        assert_eq!(send_to_path(&sock, "", "body").unwrap_err(), "bad request");
        let _ = std::fs::remove_file(&sock);
    }
}
