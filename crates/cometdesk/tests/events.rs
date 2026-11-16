//! `EventSession` over `/api/ws` against the `cometdesk-mock` device.

use std::sync::Arc;
use std::time::Duration;

use cometdesk::events::{ConnState, DeviceState, EventOptions, EventSession};
use cometdesk::hid::MouseButton;
use cometdesk::tls::PinStore;
use cometdesk::{Credentials, Error, Session};
use cometdesk_mock::{Config, MockDevice, PASSWORD, TOTP, USER};
use tokio::sync::watch;
use tokio::time::timeout;

async fn login(dev: &MockDevice, creds: Credentials) -> Session {
    Session::login(&dev.host(), &creds, Arc::new(PinStore::in_memory()))
        .await
        .unwrap()
}

async fn start(dev: &MockDevice, stream: bool) -> EventSession {
    let s = login(dev, Credentials::new(USER, PASSWORD)).await;
    EventSession::spawn(
        s,
        EventOptions {
            stream,
            ..Default::default()
        },
    )
}

async fn wait<T: Clone>(rx: &mut watch::Receiver<T>, f: impl FnMut(&T) -> bool) {
    timeout(Duration::from_secs(5), rx.wait_for(f))
        .await
        .expect("timed out")
        .expect("channel closed");
}

async fn until(f: impl Fn() -> bool) {
    timeout(Duration::from_secs(5), async {
        while !f() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("condition not reached");
}

fn connected(c: &ConnState) -> bool {
    *c == ConnState::Connected
}

#[tokio::test]
async fn connects_and_merges_initial_state() {
    let dev = MockDevice::start(Config::default()).await;
    let es = start(&dev, true).await;
    wait(&mut es.conn_state(), connected).await;
    let mut st = es.device_state();
    wait(&mut st, |s: &DeviceState| {
        s.hid().is_some() && s.atx().is_some() && s.streamer().is_some()
    })
    .await;
}

#[tokio::test]
async fn streamer_runs_only_while_a_viewer_is_open() {
    let dev = MockDevice::start(Config::default()).await;
    let idle = start(&dev, false).await;
    let mut st = idle.device_state();
    wait(&mut st, |s: &DeviceState| s.streamer().is_some()).await;
    assert!(st.borrow().streamer().unwrap().streamer.is_none());
    assert_eq!(dev.viewers(), 0);

    let live = start(&dev, true).await;
    let mut st = live.device_state();
    wait(&mut st, |s: &DeviceState| {
        s.streamer()
            .and_then(|s| s.streamer)
            .is_some_and(|r| r.source.captured_fps > 0)
    })
    .await;
    assert_eq!(dev.viewers(), 1);
    live.close().await;
    until(|| dev.viewers() == 0).await;
}

#[tokio::test]
async fn diff_events_merge_per_top_level_key() {
    let dev = MockDevice::start(Config::default()).await;
    let es = start(&dev, true).await;
    let mut st = es.device_state();
    wait(&mut st, |s: &DeviceState| s.atx().is_some()).await;
    dev.push_event("atx", serde_json::json!({"busy": true}));
    wait(&mut st, |s: &DeviceState| {
        s.raw("atx").is_some_and(|v| v["busy"] == true)
    })
    .await;
    let atx = st.borrow().raw("atx").unwrap().clone();
    assert!(atx.get("leds").is_some(), "untouched keys survive: {atx}");
}

#[tokio::test]
async fn hid_commands_reach_the_device_in_order() {
    let dev = MockDevice::start(Config::default()).await;
    let es = start(&dev, true).await;
    wait(&mut es.conn_state(), connected).await;
    let hid = es.hid();
    hid.key("KeyA", true);
    hid.key("KeyA", false);
    hid.button(MouseButton::Left, true);
    until(|| dev.hid_frames().len() >= 3).await;
    let frames = dev.hid_frames();
    let types: Vec<u8> = frames.iter().map(|f| f[0]).collect();
    assert_eq!(types, [1, 1, 2]);
    assert_eq!(&frames[0][2..], b"KeyA");
    assert_eq!(frames[0][1] & 1, 1, "down is pressed");
    assert_eq!(frames[1][1] & 1, 0, "up is released");
}

#[tokio::test]
async fn kickout_ends_without_reconnect() {
    let dev = MockDevice::start(Config::default()).await;
    let es = start(&dev, true).await;
    wait(&mut es.conn_state(), connected).await;
    dev.kickout();
    wait(&mut es.conn_state(), |c| *c == ConnState::Kicked).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(dev.ws_sessions(), 1);
}

#[tokio::test]
async fn lost_socket_reconnects() {
    let dev = MockDevice::start(Config::default()).await;
    let es = start(&dev, true).await;
    let mut conn = es.conn_state();
    wait(&mut conn, connected).await;
    dev.drop_sockets();
    wait(&mut conn, |c| matches!(c, ConnState::Disconnected(_))).await;
    wait(&mut conn, connected).await;
    assert_eq!(dev.ws_sessions(), 2);
}

#[tokio::test]
async fn expired_token_on_ws_relogs_in_once() {
    let dev = MockDevice::start(Config::default()).await;
    let s = login(&dev, Credentials::new(USER, PASSWORD)).await;
    dev.expire_token();
    let es = EventSession::spawn(s, EventOptions::default());
    wait(&mut es.conn_state(), connected).await;
    assert_eq!(dev.logins(), 2);
}

#[tokio::test]
async fn ws_without_relogin_is_unauthorized() {
    let dev = MockDevice::start(Config {
        twofa: true,
        ..Default::default()
    })
    .await;
    let s = login(&dev, Credentials::new(USER, PASSWORD).with_totp(TOTP)).await;
    dev.expire_token();
    assert!(matches!(
        s.ws_connect("api/ws").await,
        Err(Error::Unauthorized)
    ));
}

#[tokio::test]
async fn logout_closes_the_event_socket() {
    let dev = MockDevice::start(Config::default()).await;
    let s = login(&dev, Credentials::new(USER, PASSWORD)).await;
    let es = EventSession::spawn(s.clone(), EventOptions::default());
    wait(&mut es.conn_state(), connected).await;
    s.logout().await.unwrap();
    wait(&mut es.conn_state(), |c| !connected(c)).await;
}

#[tokio::test]
async fn abrupt_client_drops_release_viewers() {
    let dev = MockDevice::start(Config::default()).await;
    let s = login(&dev, Credentials::new(USER, PASSWORD)).await;
    let mut sockets = Vec::new();
    for _ in 0..40 {
        sockets.push(s.ws_connect("api/ws").await.unwrap());
    }
    until(|| dev.viewers() == 40).await;
    drop(sockets); // no close handshake: the TCP/TLS streams just vanish
    until(|| dev.viewers() == 0).await;
}

#[tokio::test]
async fn event_flood_ends_with_the_latest_state() {
    let dev = MockDevice::start(Config::default()).await;
    let es = start(&dev, true).await;
    wait(&mut es.conn_state(), connected).await;
    // The initial full state must land first, or it would overwrite the pushed diff.
    wait(&mut es.device_state(), |s: &DeviceState| {
        s.streamer().is_some()
    })
    .await;
    let blob = "x".repeat(2000);
    for i in 0..2000 {
        dev.push_event("flood", serde_json::json!({"n": i, "pad": blob}));
    }
    dev.push_event(
        "streamer",
        serde_json::json!({"streamer": {"hdmi": {"signal": false}}}),
    );
    // The 64-slot broadcast channel drops old events for a slow reader, never the newest.
    wait(&mut es.device_state(), |s: &DeviceState| {
        s.video_signal() == Some(false)
    })
    .await;
}

#[tokio::test]
async fn hid_flood_is_delivered_in_order() {
    let dev = MockDevice::start(Config::default()).await;
    let es = start(&dev, false).await;
    wait(&mut es.conn_state(), connected).await;
    for _ in 0..500 {
        es.hid().key("KeyA", true);
        es.hid().key("KeyA", false);
    }
    es.hid().key("KeyZ", true);
    until(|| {
        dev.hid_frames()
            .last()
            .is_some_and(|f| f.ends_with(b"KeyZ"))
    })
    .await;
    let frames = dev.hid_frames();
    assert!(frames.iter().all(|f| f[0] == 1), "key frames only");
}
