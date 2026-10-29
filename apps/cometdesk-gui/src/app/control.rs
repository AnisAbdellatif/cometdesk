//! Automation: a local control socket so scripts and agents can drive the app.
//!
//! `COMETDESK_CONTROL=1` (or a socket path) opens a Unix socket speaking one JSON object per
//! line, each answered by one JSON line. The socket gives full control of the target's
//! keyboard and mouse, so it is opt-in and owner-only (0600). See `docs/automation.md`.

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use serde_json::{Value, json};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};

use super::{App, UserEvent};

pub struct Request {
    pub cmd: Value,
    pub reply: Sender<Value>,
}

/// Where the socket goes: `COMETDESK_CONTROL` when it is a path, else the config directory.
#[cfg(unix)]
fn socket_path() -> Option<PathBuf> {
    match std::env::var("COMETDESK_CONTROL")
        .ok()
        .filter(|v| !v.is_empty())?
        .as_str()
    {
        "0" => None,
        "1" => cometdesk::tls::app_dir().map(|d| d.join("control.sock")),
        path => Some(path.into()),
    }
}

#[cfg(unix)]
pub fn start(proxy: EventLoopProxy<UserEvent>) {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::time::Duration;

    let Some(path) = socket_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if UnixStream::connect(&path).is_ok() {
        tracing::error!(path = %path.display(), "control socket is in use by another instance");
        return;
    }
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => return tracing::error!(%e, path = %path.display(), "cannot open control socket"),
    };
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    tracing::info!(path = %path.display(), "control socket ready");

    std::thread::Builder::new()
        .name("cometdesk-control".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let proxy = proxy.clone();
                std::thread::spawn(move || {
                    let Ok(mut out) = stream.try_clone() else {
                        return;
                    };
                    for line in BufReader::new(stream).lines().map_while(Result::ok) {
                        if line.trim().is_empty() {
                            continue;
                        }
                        let reply = match serde_json::from_str::<Value>(&line) {
                            Err(e) => json!({"ok": false, "error": format!("bad JSON: {e}")}),
                            Ok(cmd) => {
                                let id = cmd.get("id").cloned();
                                let (tx, rx) = std::sync::mpsc::channel();
                                if proxy
                                    .send_event(UserEvent::Control(Request { cmd, reply: tx }))
                                    .is_err()
                                {
                                    return; // the app is gone
                                }
                                let mut r = rx
                                    .recv_timeout(Duration::from_secs(60))
                                    .unwrap_or_else(|_| json!({"ok": false, "error": "timed out"}));
                                if let (Some(id), Some(o)) = (id, r.as_object_mut()) {
                                    o.insert("id".into(), id);
                                }
                                r
                            }
                        };
                        if writeln!(out, "{reply}").is_err() {
                            return;
                        }
                    }
                });
            }
        })
        .ok();
}

#[cfg(not(unix))]
pub fn start(_: EventLoopProxy<UserEvent>) {}

/// Outcome of a command: answered now, or answered later by whoever took `reply`.
enum Out {
    Now(Value),
    Later,
}

fn ok() -> Result<Out, String> {
    Ok(Out::Now(json!({"ok": true})))
}

fn str_arg<'a>(cmd: &'a Value, key: &str) -> Result<&'a str, String> {
    cmd.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string `{key}`"))
}

fn num(cmd: &Value, key: &str) -> Option<f64> {
    cmd.get(key).and_then(Value::as_f64)
}

/// Key names come from callers and `HidCommand` wants `&'static str`; keep one copy of each.
fn key_name(name: &str) -> Result<&'static str, String> {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    static NAMES: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    if name.is_empty() || name.len() > 32 || !name.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!(
            "bad key name `{name}` (W3C code such as KeyA, Enter, ControlLeft)"
        ));
    }
    let mut set = NAMES.get_or_init(Default::default).lock().unwrap();
    if let Some(n) = set.get(name) {
        return Ok(n);
    }
    let n: &'static str = Box::leak(name.to_owned().into_boxed_str());
    set.insert(n);
    Ok(n)
}

fn button(cmd: &Value) -> Result<cometdesk::hid::MouseButton, String> {
    use cometdesk::hid::MouseButton::*;
    Ok(
        match cmd.get("button").and_then(Value::as_str).unwrap_or("left") {
            "left" => Left,
            "right" => Right,
            "middle" => Middle,
            "back" => Back,
            "forward" => Forward,
            b => return Err(format!("unknown button `{b}`")),
        },
    )
}

fn out_path(cmd: &Value, name: &str, ext: &str) -> PathBuf {
    match cmd.get("path").and_then(Value::as_str) {
        Some(p) => p.into(),
        None => {
            let n = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis());
            std::env::temp_dir().join(format!("cometdesk-{name}-{n}.{ext}"))
        }
    }
}

impl App {
    pub(super) fn control(&mut self, el: &ActiveEventLoop, req: Request) {
        match self.dispatch(el, &req.cmd, &req.reply) {
            Ok(Out::Now(v)) => {
                let _ = req.reply.send(v);
            }
            Ok(Out::Later) => {}
            Err(e) => {
                let _ = req.reply.send(json!({"ok": false, "error": e}));
            }
        }
    }

    fn dispatch(
        &mut self,
        el: &ActiveEventLoop,
        cmd: &Value,
        reply: &Sender<Value>,
    ) -> Result<Out, String> {
        use cometdesk::hid::{HidCommand, to_abs};
        let name = str_arg(cmd, "cmd")?;
        // Commands that need no connection.
        match name {
            "status" => return Ok(Out::Now(self.control_status())),
            "devices" => {
                let names: Vec<_> = self
                    .devices
                    .store
                    .devices
                    .iter()
                    .map(|d| d.display_name())
                    .collect();
                return Ok(Out::Now(json!({"ok": true, "devices": names})));
            }
            "connect" => {
                let want = str_arg(cmd, "name")?;
                let device = self
                    .devices
                    .store
                    .devices
                    .iter()
                    .find(|d| d.name == want || d.host == want)
                    .cloned()
                    .ok_or_else(|| format!("no saved device `{want}`"))?;
                if self.console.is_some() {
                    return Err("already connected; disconnect first".into());
                }
                self.devices.connect_device(device);
                self.request_redraw();
                return ok();
            }
            "disconnect" => {
                self.handle_action(super::ConsoleAction::Disconnect);
                return ok();
            }
            "quit" => {
                if let Some(c) = self.console.take() {
                    c.disconnect();
                }
                el.exit();
                return ok();
            }
            "screenshot" => {
                let path = out_path(cmd, "window", "png");
                let (tx, rx) = std::sync::mpsc::channel();
                let gpu = self.gpu.as_mut().ok_or("no window yet")?;
                gpu.request_capture(tx);
                self.request_redraw();
                let reply = reply.clone();
                std::thread::spawn(move || {
                    let r = match rx.recv_timeout(std::time::Duration::from_secs(10)) {
                        Ok((w, h, rgba)) => cometdesk_clip::encode_png(w, h, &rgba)
                            .map_err(|e| e.to_string())
                            .and_then(|png| std::fs::write(&path, png).map_err(|e| e.to_string()))
                            .map(|()| json!({"ok": true, "path": path, "width": w, "height": h})),
                        Err(_) => Err("no frame was drawn (window hidden?)".into()),
                    };
                    let _ = reply.send(r.unwrap_or_else(|e| json!({"ok": false, "error": e})));
                });
                return Ok(Out::Later);
            }
            "ui_click" | "ui_text" => {
                let state = self.egui_state.as_mut().ok_or("no window yet")?;
                let ev = &mut state.egui_input_mut().events;
                if name == "ui_text" {
                    ev.push(egui::Event::Text(str_arg(cmd, "text")?.to_owned()));
                } else {
                    let pos = egui::pos2(
                        num(cmd, "x").ok_or("missing `x` (window points)")? as f32,
                        num(cmd, "y").ok_or("missing `y`")? as f32,
                    );
                    ev.push(egui::Event::PointerMoved(pos));
                    for pressed in [true, false] {
                        ev.push(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        });
                    }
                }
                self.request_redraw();
                return ok();
            }
            _ => {}
        }

        let console = self.console.as_mut().ok_or("not connected to a device")?;
        // Position on the target: `x`,`y` in 0..1, or `px`,`py` in target pixels.
        let target_pos = |size: Option<(u32, u32)>| -> Result<Option<(i16, i16)>, String> {
            if let (Some(x), Some(y)) = (num(cmd, "x"), num(cmd, "y")) {
                return Ok(Some((to_abs(x as f32), to_abs(y as f32))));
            }
            if let (Some(x), Some(y)) = (num(cmd, "px"), num(cmd, "py")) {
                let (w, h) = size.ok_or("no video frame yet; use x/y in 0..1")?;
                return Ok(Some((
                    to_abs((x / f64::from(w)) as f32),
                    to_abs((y / f64::from(h)) as f32),
                )));
            }
            Ok(None)
        };
        let size = self.gpu.as_ref().and_then(|g| g.video.video_size());
        match name {
            "key" => {
                let code = key_name(str_arg(cmd, "code")?)?;
                let action = cmd.get("action").and_then(Value::as_str).unwrap_or("tap");
                let mut v = Vec::new();
                if action != "up" {
                    v.push(HidCommand::Key {
                        code,
                        pressed: true,
                    });
                }
                if action != "down" {
                    v.push(HidCommand::Key {
                        code,
                        pressed: false,
                    });
                }
                console.send_hid(v);
            }
            "combo" => {
                let keys = cmd
                    .get("keys")
                    .and_then(Value::as_array)
                    .ok_or("missing array `keys`")?
                    .iter()
                    .map(|k| key_name(k.as_str().unwrap_or("")))
                    .collect::<Result<Vec<_>, _>>()?;
                console.send_hid(crate::actions::combo(&keys));
            }
            "type" => console.control_type(str_arg(cmd, "text")?.to_owned()),
            "mouse_move" => {
                let (x, y) = target_pos(size)?.ok_or("need x,y (0..1) or px,py")?;
                console.send_hid([HidCommand::MoveAbs { x, y }]);
            }
            "mouse_button" => {
                let pressed = cmd.get("action").and_then(Value::as_str) != Some("up");
                console.send_hid([HidCommand::Button {
                    button: button(cmd)?,
                    pressed,
                }]);
            }
            "click" => {
                let button = button(cmd)?;
                let mut v = Vec::new();
                if let Some((x, y)) = target_pos(size)? {
                    v.push(HidCommand::MoveAbs { x, y });
                }
                for _ in 0..cmd.get("count").and_then(Value::as_u64).unwrap_or(1).min(3) {
                    v.push(HidCommand::Button {
                        button,
                        pressed: true,
                    });
                    v.push(HidCommand::Button {
                        button,
                        pressed: false,
                    });
                }
                console.send_hid(v);
            }
            "wheel" => {
                let d = |k| num(cmd, k).unwrap_or(0.0).clamp(-127.0, 127.0) as i8;
                console.send_hid([HidCommand::Wheel {
                    dx: d("dx"),
                    dy: d("dy"),
                }]);
            }
            "release_all" => console.release_all(),
            // The device's own snapshot of the target (JPEG), optionally with its text (macOS).
            "snapshot" | "ocr" => {
                console.control_snapshot(
                    out_path(cmd, "target", "jpg"),
                    name == "ocr",
                    reply.clone(),
                );
                return Ok(Out::Later);
            }
            other => return Err(format!("unknown command `{other}`")),
        }
        ok()
    }

    fn control_status(&self) -> Value {
        let screen = match (&self.console, &self.connecting) {
            (Some(_), _) => "console",
            (None, Some(_)) => "connecting",
            _ => "devices",
        };
        json!({
            "ok": true,
            "screen": screen,
            "connecting": self.connecting,
            "window": self.window.as_ref().map(|w| {
                let s = w.inner_size();
                json!({"width": s.width, "height": s.height, "scale": w.scale_factor(), "focused": self.focused})
            }),
            "video": self.gpu.as_ref().and_then(|g| g.video.video_size()).map(|(w, h)| json!({"width": w, "height": h})),
            "console": self.console.as_ref().map(|c| c.control_status()),
        })
    }
}
