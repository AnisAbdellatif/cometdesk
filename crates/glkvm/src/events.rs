//! The `/api/ws` session: device events in, HID input out.
//!
//! One task owns the socket. HID commands arrive on an unbounded channel and go through a
//! [`Coalescer`], so ordering is preserved and mouse moves are rate-limited. All frames
//! produced in one wakeup are written with a single flush. The connection is re-established
//! with exponential backoff; HID typed while disconnected is dropped, not replayed.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior, sleep, sleep_until};
use tokio_tungstenite::tungstenite::Message;

use crate::api::atx::AtxState;
use crate::api::hid::HidState;
use crate::api::streamer::StreamerState;
use crate::client::{Session, WsStream};
use crate::hid::{Coalescer, HidCommand, MouseButton};

const PING_INTERVAL: Duration = Duration::from_secs(2);
const PONG_TIMEOUT: Duration = Duration::from_secs(6);
const BACKOFF_MIN: Duration = Duration::from_millis(250);
const BACKOFF_MAX: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnState {
    Connecting,
    Connected,
    /// Lost; reconnecting. Holds the reason.
    Disconnected(String),
    /// The device ended this session (`kickout`); no reconnect.
    Kicked,
    /// [`EventSession::close`] was called.
    Closed,
}

/// One device event: `event_type` and its payload.
#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    #[serde(rename = "event_type")]
    pub kind: String,
    #[serde(rename = "event", default)]
    pub data: serde_json::Value,
}

/// Latest device state, built from events.
///
/// kvmd sends the full state of a subsystem first, then only the top-level keys that changed
/// (a `streamer` event may carry just `{"streamer": {...}}` without `features`/`params`), so
/// payloads are merged key by key, not replaced.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceState {
    raw: HashMap<String, serde_json::Value>,
}

impl DeviceState {
    /// Merged payload of one event type (`streamer`, `hid`, `atx`, `info`, ...).
    pub fn raw(&self, kind: &str) -> Option<&serde_json::Value> {
        self.raw.get(kind)
    }

    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    pub fn hid(&self) -> Option<HidState> {
        self.typed("hid")
    }

    pub fn atx(&self) -> Option<AtxState> {
        self.typed("atx")
    }

    pub fn streamer(&self) -> Option<StreamerState> {
        self.typed("streamer")
    }

    /// `Some(false)` when the streamer runs but the target sends no picture (off/asleep).
    pub fn video_signal(&self) -> Option<bool> {
        self.streamer()?.streamer.map(|rt| rt.hdmi.signal)
    }

    fn typed<T: serde::de::DeserializeOwned>(&self, kind: &str) -> Option<T> {
        let v = self.raw.get(kind)?;
        T::deserialize(v)
            .inspect_err(|e| tracing::debug!(kind, %e, "unexpected event shape"))
            .ok()
    }

    fn apply(&mut self, kind: &str, data: &serde_json::Value) {
        use serde_json::Value;
        match (self.raw.get_mut(kind), data) {
            (Some(Value::Object(current)), Value::Object(update)) => {
                for (k, v) in update {
                    current.insert(k.clone(), v.clone());
                }
            }
            _ => {
                self.raw.insert(kind.to_owned(), data.clone());
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EventOptions {
    /// Keep the video streamer alive. `false` sends `stream=0`.
    pub stream: bool,
    pub move_interval: Duration,
}

impl Default for EventOptions {
    fn default() -> Self {
        Self {
            stream: true,
            move_interval: crate::hid::DEFAULT_MOVE_INTERVAL,
        }
    }
}

/// Cheap, cloneable handle for sending input. Never blocks.
#[derive(Debug, Clone)]
pub struct HidSender {
    tx: mpsc::UnboundedSender<HidCommand>,
}

impl HidSender {
    pub fn send(&self, cmd: HidCommand) {
        let _ = self.tx.send(cmd);
    }
    pub fn key(&self, code: &'static str, pressed: bool) {
        self.send(HidCommand::Key { code, pressed });
    }
    pub fn button(&self, button: MouseButton, pressed: bool) {
        self.send(HidCommand::Button { button, pressed });
    }
    pub fn move_abs(&self, x: i16, y: i16) {
        self.send(HidCommand::MoveAbs { x, y });
    }
    pub fn wheel(&self, dx: i8, dy: i8) {
        self.send(HidCommand::Wheel { dx, dy });
    }
    pub fn release_all(&self) {
        self.send(HidCommand::ReleaseAll);
    }
}

pub struct EventSession {
    hid: HidSender,
    conn: watch::Receiver<ConnState>,
    state: watch::Receiver<DeviceState>,
    events: broadcast::Sender<Event>,
    close: Option<tokio::sync::oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

impl EventSession {
    /// Starts the session on the current tokio runtime.
    pub fn spawn(session: Session, opts: EventOptions) -> EventSession {
        let (tx, rx) = mpsc::unbounded_channel();
        let (conn_tx, conn) = watch::channel(ConnState::Connecting);
        let (state_tx, state) = watch::channel(DeviceState::default());
        let (events, _) = broadcast::channel(256);
        let (close_tx, close_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(supervise(
            session,
            opts,
            rx,
            conn_tx,
            state_tx,
            events.clone(),
            close_rx,
        ));
        EventSession {
            hid: HidSender { tx },
            conn,
            state,
            events,
            close: Some(close_tx),
            task,
        }
    }

    pub fn hid(&self) -> HidSender {
        self.hid.clone()
    }

    pub fn conn_state(&self) -> watch::Receiver<ConnState> {
        self.conn.clone()
    }

    pub fn device_state(&self) -> watch::Receiver<DeviceState> {
        self.state.clone()
    }

    /// Every event. A slow receiver loses events (never state: see [`Self::device_state`]).
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    /// Releases held input, closes the socket and waits for the task to end.
    pub async fn close(mut self) {
        self.hid.release_all();
        if let Some(tx) = self.close.take() {
            let _ = tx.send(());
        }
        let _ = (&mut self.task).await;
    }
}

impl Drop for EventSession {
    fn drop(&mut self) {
        if self.close.is_some() {
            self.task.abort();
        }
    }
}

enum Exit {
    Lost(String),
    Kicked,
    Closed,
}

async fn supervise(
    session: Session,
    opts: EventOptions,
    mut rx: mpsc::UnboundedReceiver<HidCommand>,
    conn_tx: watch::Sender<ConnState>,
    state_tx: watch::Sender<DeviceState>,
    events: broadcast::Sender<Event>,
    mut close_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let path = if opts.stream {
        "api/ws"
    } else {
        "api/ws?stream=0"
    };
    let mut backoff = BACKOFF_MIN;
    let mut coalescer = Coalescer::new(opts.move_interval);
    loop {
        conn_tx.send_replace(ConnState::Connecting);
        let connected = tokio::select! {
            r = session.ws_connect(path) => r,
            _ = &mut close_rx => { conn_tx.send_replace(ConnState::Closed); return; }
        };
        let exit = match connected {
            Ok(ws) => {
                backoff = BACKOFF_MIN;
                // Input typed while we were down is stale.
                while rx.try_recv().is_ok() {}
                coalescer.reset();
                conn_tx.send_replace(ConnState::Connected);
                run(
                    ws,
                    &mut rx,
                    &mut coalescer,
                    &state_tx,
                    &events,
                    &mut close_rx,
                )
                .await
            }
            Err(e) => Exit::Lost(e.to_string()),
        };
        match exit {
            Exit::Closed => {
                conn_tx.send_replace(ConnState::Closed);
                return;
            }
            Exit::Kicked => {
                conn_tx.send_replace(ConnState::Kicked);
                return;
            }
            Exit::Lost(reason) => {
                tracing::warn!(%reason, retry_in = ?backoff, "event socket lost");
                conn_tx.send_replace(ConnState::Disconnected(reason));
                tokio::select! {
                    _ = sleep(backoff) => {}
                    _ = &mut close_rx => { conn_tx.send_replace(ConnState::Closed); return; }
                }
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
        }
    }
}

async fn run(
    ws: WsStream,
    rx: &mut mpsc::UnboundedReceiver<HidCommand>,
    coalescer: &mut Coalescer,
    state_tx: &watch::Sender<DeviceState>,
    events: &broadcast::Sender<Event>,
    close_rx: &mut tokio::sync::oneshot::Receiver<()>,
) -> Exit {
    let (mut sink, mut stream) = ws.split();
    let mut frames: Vec<Vec<u8>> = Vec::with_capacity(16);
    let mut ping = tokio::time::interval(PING_INTERVAL);
    ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut last_pong = Instant::now();
    let ping_msg = Message::text(r#"{"event_type":"ping","event":{}}"#);

    loop {
        let deadline = coalescer.deadline().map(Instant::from_std);
        tokio::select! {
            biased;
            _ = &mut *close_rx => {
                let now = std::time::Instant::now();
                coalescer.push(HidCommand::ReleaseAll, now, &mut frames);
                for f in frames.drain(..) {
                    let _ = sink.feed(Message::binary(f)).await;
                }
                let _ = sink.close().await;
                return Exit::Closed;
            }
            cmd = rx.recv() => {
                let Some(cmd) = cmd else { return Exit::Closed };
                let now = std::time::Instant::now();
                coalescer.push(cmd, now, &mut frames);
                // Batch everything that is already queued into one flush.
                while let Ok(cmd) = rx.try_recv() {
                    coalescer.push(cmd, now, &mut frames);
                }
            }
            _ = sleep_until(deadline.unwrap_or_else(Instant::now)), if deadline.is_some() => {
                coalescer.poll(std::time::Instant::now(), &mut frames);
            }
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match handle_text(&text, state_tx, events) {
                            Incoming::Pong => last_pong = Instant::now(),
                            Incoming::Kickout => return Exit::Kicked,
                            Incoming::Other => {}
                        }
                    }
                    Some(Ok(Message::Close(frame))) => {
                        return Exit::Lost(format!("closed by device: {frame:?}"));
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Exit::Lost(e.to_string()),
                    None => return Exit::Lost("socket ended".into()),
                }
            }
            _ = ping.tick() => {
                if last_pong.elapsed() > PONG_TIMEOUT {
                    return Exit::Lost("no pong from device".into());
                }
                if let Err(e) = sink.feed(ping_msg.clone()).await {
                    return Exit::Lost(e.to_string());
                }
                if let Err(e) = sink.flush().await {
                    return Exit::Lost(e.to_string());
                }
            }
        }
        if !frames.is_empty() {
            for f in frames.drain(..) {
                if let Err(e) = sink.feed(Message::binary(f)).await {
                    return Exit::Lost(e.to_string());
                }
            }
            if let Err(e) = sink.flush().await {
                return Exit::Lost(e.to_string());
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Incoming {
    Pong,
    Kickout,
    Other,
}

fn handle_text(
    text: &str,
    state_tx: &watch::Sender<DeviceState>,
    events: &broadcast::Sender<Event>,
) -> Incoming {
    let Ok(event) = serde_json::from_str::<Event>(text) else {
        tracing::debug!(len = text.len(), "unparseable event");
        return Incoming::Other;
    };
    match event.kind.as_str() {
        "pong" => return Incoming::Pong,
        "kickout" => return Incoming::Kickout,
        _ => {}
    }
    state_tx.send_modify(|s| s.apply(&event.kind, &event.data));
    let _ = events.send(event);
    Incoming::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_update_state_and_broadcast() {
        let (state_tx, state_rx) = watch::channel(DeviceState::default());
        let (events, mut sub) = broadcast::channel(4);
        let r = handle_text(
            r#"{"event_type":"streamer","event":{"source":{"online":true}}}"#,
            &state_tx,
            &events,
        );
        assert_eq!(r, Incoming::Other);
        assert_eq!(
            state_rx.borrow().raw("streamer").unwrap()["source"]["online"],
            true
        );
        assert_eq!(sub.try_recv().unwrap().kind, "streamer");
    }

    #[test]
    fn pong_and_kickout_are_control_events() {
        let (state_tx, state_rx) = watch::channel(DeviceState::default());
        let (events, _) = broadcast::channel(4);
        assert_eq!(
            handle_text(r#"{"event_type":"pong","event":{}}"#, &state_tx, &events),
            Incoming::Pong
        );
        assert_eq!(
            handle_text(r#"{"event_type":"kickout","event":{}}"#, &state_tx, &events),
            Incoming::Kickout
        );
        assert!(state_rx.borrow().is_empty());
    }

    fn feed(state_tx: &watch::Sender<DeviceState>, kind: &str, data: serde_json::Value) {
        let (events, _) = broadcast::channel(4);
        let text = serde_json::json!({"event_type": kind, "event": data}).to_string();
        handle_text(&text, state_tx, &events);
    }

    #[test]
    fn partial_updates_are_merged_per_top_level_key() {
        let (tx, rx) = watch::channel(DeviceState::default());
        // Full state first (as recorded from the device), then a diff with only `streamer`.
        let full = crate::api::fixtures::load::<serde_json::Value>("streamer_idle");
        feed(&tx, "streamer", full);
        let running = crate::api::fixtures::load::<serde_json::Value>("streamer_running");
        feed(
            &tx,
            "streamer",
            serde_json::json!({"streamer": running["streamer"]}),
        );

        let s = rx.borrow().streamer().expect("typed streamer");
        assert!(s.features.h264, "features kept from the first event");
        assert_eq!(s.params.venc_mode.as_deref(), Some("smart"));
        assert!(s.streamer.expect("runtime from the diff").h264.online);
        assert_eq!(rx.borrow().video_signal(), Some(true));
    }

    #[test]
    fn typed_hid_and_atx() {
        let (tx, rx) = watch::channel(DeviceState::default());
        feed(&tx, "hid", crate::api::fixtures::load("hid"));
        feed(&tx, "atx", crate::api::fixtures::load("atx"));
        let state = rx.borrow();
        assert!(state.hid().unwrap().keyboard.online);
        assert!(!state.atx().unwrap().enabled);
        assert_eq!(state.video_signal(), None, "no streamer event yet");
    }

    #[test]
    fn non_object_payload_replaces() {
        let (tx, rx) = watch::channel(DeviceState::default());
        feed(&tx, "x", serde_json::json!({"a": 1}));
        feed(&tx, "x", serde_json::json!([1, 2]));
        assert_eq!(rx.borrow().raw("x"), Some(&serde_json::json!([1, 2])));
    }

    #[test]
    fn garbage_is_ignored() {
        let (state_tx, _) = watch::channel(DeviceState::default());
        let (events, _) = broadcast::channel(4);
        assert_eq!(handle_text("not json", &state_tx, &events), Incoming::Other);
        assert_eq!(
            handle_text(r#"{"event":{}}"#, &state_tx, &events),
            Incoming::Other
        );
    }
}
