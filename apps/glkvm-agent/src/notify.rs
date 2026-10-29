//! Lets scripts on the target raise a notification on the user's Mac:
//! `glkvm-agent notify "Build finished" "took 4 min"`. The command talks to the running
//! agent over the loopback port that also serves as the single-instance lock; the agent
//! forwards the message to the GUI while it is connected.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::{INSTANCE_PORT, log};

/// Longest request line we read; anything longer is cut by the protocol anyway.
const MAX_REQUEST: u64 = 4096;
const IO_TIMEOUT: Duration = Duration::from_secs(2);
/// Anyone on the target can use the port, so a runaway script cannot flood the Mac.
const BURST: u32 = 5;
const REFILL: Duration = Duration::from_secs(2);

pub struct Note {
    pub title: String,
    pub body: String,
}

/// Serves `notify` requests on `listener` (the instance lock) from a background thread.
pub fn serve(listener: TcpListener, tx: mpsc::Sender<Note>) {
    std::thread::Builder::new()
        .name("notify".into())
        .spawn(move || {
            let (mut tokens, mut at) = (BURST, Instant::now());
            for conn in listener.incoming().flatten() {
                let refill = (at.elapsed().as_secs_f64() / REFILL.as_secs_f64()) as u32;
                if refill > 0 {
                    tokens = (tokens + refill).min(BURST);
                    at = Instant::now();
                }
                let reply = match read_note(&conn) {
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
        })
        .expect("spawn notify thread");
}

/// Request: `notify\n<title>\n<body>` (the body may span lines), then the sender closes its
/// writing side.
fn read_note(conn: &TcpStream) -> Option<Note> {
    conn.set_read_timeout(Some(IO_TIMEOUT)).ok()?;
    let mut text = String::new();
    conn.take(MAX_REQUEST).read_to_string(&mut text).ok()?;
    let mut parts = text.splitn(3, '\n');
    (parts.next()? == "notify").then_some(())?;
    let title = parts.next()?.trim().to_owned();
    let body = parts.next().unwrap_or("").trim().to_owned();
    (!title.is_empty()).then_some(Note { title, body })
}

/// The `notify` command: hands the message to the running agent.
pub fn send(title: &str, body: &str) -> Result<(), String> {
    send_to(INSTANCE_PORT, title, body)
}

fn send_to(port: u16, title: &str, body: &str) -> Result<(), String> {
    let mut conn =
        TcpStream::connect_timeout(&(std::net::Ipv4Addr::LOCALHOST, port).into(), IO_TIMEOUT)
            .map_err(|_| "the agent is not running".to_string())?;
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

    #[test]
    fn requests_reach_the_agent_and_floods_are_refused() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        serve(listener, tx);
        send_to(port, "Build", "done\nin 4 min").unwrap();
        let note = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            (note.title.as_str(), note.body.as_str()),
            ("Build", "done\nin 4 min")
        );
        for _ in 1..BURST {
            send_to(port, "x", "").unwrap();
        }
        assert_eq!(
            send_to(port, "x", "").unwrap_err(),
            "too many notifications"
        );
        assert_eq!(send_to(port, "", "body").unwrap_err(), "bad request");
    }
}
