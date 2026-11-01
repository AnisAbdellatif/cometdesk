//! Single connected relay session loop and `cometdesk_clip::Event` dispatch for `cometdesk-agent`.
//! Entry point: `session`.

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use cometdesk_clip::{Content, Endpoint, Error, Event, FileReceiver, Hello, Policy, Role, Secret};

use crate::clipboard::{ClipCommand, Local};
use crate::{
    browse, capture, config, display, filesend, info, insert, log, notify, power, update, volume,
};

/// The GUI pings every 15 s once the session is up.
const IDLE_TIMEOUT: Duration = Duration::from_secs(45);
const READ_SLICE: Duration = Duration::from_millis(100);
/// Notify the user when an unplugged target drops to this percentage.
const BATTERY_LOW: u8 = 15;

pub(crate) enum SessionEnd {
    Mismatch,
    /// Reason, and whether a session had been established (resets the backoff).
    Lost(String, bool),
    ClipboardGone,
}

pub(crate) fn session(
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
    let mut files = FileReceiver::new(config::staging_dir());
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
                            let update = files.is_update(id);
                            let mut result = files.end(id, intact);
                            let mut restart_as = None;
                            if update {
                                result = result
                                    .and_then(|staged| {
                                        update::install(std::path::Path::new(&staged))
                                    })
                                    .map(|exe| {
                                        restart_as = Some(exe);
                                        "installed; restarting".to_owned()
                                    });
                            }
                            log!("file done: {result:?}");
                            ep.send_file_result(id, result)
                                .map_err(|e| SessionEnd::Lost(e.to_string(), true))?;
                            if let Some(exe) = restart_as {
                                let _ = stream.write_all(&ep.take_outgoing());
                                let args: Vec<String> = std::env::args().skip(1).collect();
                                log!("restarting as the updated agent");
                                update::restart(&exe, &args);
                            }
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
                        Event::SetVolume { id, level, muted } => {
                            let result = volume::set(level, muted);
                            log!("volume {level:?} muted {muted:?}: {result:?}");
                            probe.refresh();
                            ep.send_ack(id, result)
                                .map_err(|e| SessionEnd::Lost(e.to_string(), true))?;
                        }
                        Event::SetDisplay {
                            id,
                            display,
                            mode,
                            primary,
                        } => {
                            let result = display::set(display, mode, primary);
                            log!("display {display} {mode:?} primary {primary}: {result:?}");
                            probe.refresh();
                            ep.send_ack(id, result)
                                .map_err(|e| SessionEnd::Lost(e.to_string(), true))?;
                        }
                        Event::ListDir { id, target } => {
                            let result = browse::list(target);
                            ep.send_dir_list(id, result)
                                .map_err(|e| SessionEnd::Lost(e.to_string(), true))?;
                        }
                        Event::Fetch { id, dir, name } => {
                            let result = browse::file(&dir, &name).map(|path| {
                                log!("the GUI fetches {}", path.display());
                                outbox.fetch(&mut ep, path);
                            });
                            ep.send_ack(id, result)
                                .map_err(|e| SessionEnd::Lost(e.to_string(), true))?;
                        }
                        Event::Mismatch(peer) => log!(
                            "the GUI speaks protocol {}, this agent {}: waiting for an update",
                            peer.protocol,
                            cometdesk_clip::PROTOCOL
                        ),
                        Event::Notify { .. }
                        | Event::DirList { .. }
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
