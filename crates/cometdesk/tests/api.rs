//! REST API wrappers against the `cometdesk-mock` device (recorded rm1-v1.10.1 responses).

use std::sync::Arc;

use cometdesk::api::atx::{AtxButton, PowerAction};
use cometdesk::api::hid::PrintOptions;
use cometdesk::tls::PinStore;
use cometdesk::{Credentials, Error, Session};
use cometdesk_mock::{Config, MockDevice, PASSWORD, USER};

async fn connect(config: Config) -> (MockDevice, Session) {
    let dev = MockDevice::start(config).await;
    let session = Session::login(
        &dev.host(),
        &Credentials::new(USER, PASSWORD),
        Arc::new(PinStore::in_memory()),
    )
    .await
    .unwrap();
    (dev, session)
}

fn query(call: &cometdesk_mock::Call) -> Vec<(String, String)> {
    url::form_urlencoded::parse(call.query.as_bytes())
        .into_owned()
        .collect()
}

fn has(pairs: &[(String, String)], k: &str, v: &str) -> bool {
    pairs.iter().any(|(key, val)| key == k && val == v)
}

#[tokio::test]
async fn system_info_and_firmware_parse() {
    let (_dev, s) = connect(Config::default()).await;
    assert!(!s.system_info().await.unwrap().kvmd.version.is_empty());
    assert_eq!(s.firmware().await.unwrap().model, "RM1");
}

#[tokio::test]
async fn capabilities_match_fixture_device() {
    let (_dev, s) = connect(Config::default()).await;
    let c = s.capabilities().await.unwrap();
    assert!(!c.atx && c.msd && c.hid);
}

#[tokio::test]
async fn atx_actions_send_wire_names() {
    let (dev, s) = connect(Config {
        atx: true,
        ..Default::default()
    })
    .await;
    s.atx_power(PowerAction::OffHard, true).await.unwrap();
    s.atx_click(AtxButton::PowerLong, false).await.unwrap();
    let power = query(&dev.calls_to("/api/atx/power")[0]);
    assert!(has(&power, "action", "off_hard") && has(&power, "wait", "1"));
    let click = query(&dev.calls_to("/api/atx/click")[0]);
    assert!(has(&click, "button", "power_long") && has(&click, "wait", "0"));
}

#[tokio::test]
async fn atx_action_error_maps_to_api_error() {
    let (_dev, s) = connect(Config::default()).await;
    let r = s.atx_power(PowerAction::On, false).await;
    assert!(
        matches!(r, Err(Error::Api { status: 400, .. })),
        "{:?}",
        r.err()
    );
}

#[tokio::test]
async fn msd_mount_sets_params_then_connects() {
    let (dev, s) = connect(Config::default()).await;
    assert!(
        s.msd_state()
            .await
            .unwrap()
            .storage
            .images
            .contains_key("image-0.iso")
    );
    s.msd_mount("image-0.iso", true).await.unwrap();
    let calls: Vec<_> = dev
        .calls()
        .into_iter()
        .filter(|c| c.path.starts_with("/api/msd/set_"))
        .collect();
    assert_eq!(calls[0].path, "/api/msd/set_params");
    assert!(has(&query(&calls[0]), "image", "image-0.iso"));
    assert_eq!(calls[1].path, "/api/msd/set_connected");
    assert!(has(&query(&calls[1]), "connected", "1"));
}

#[tokio::test]
async fn streamer_is_idle_without_a_viewer() {
    let (_dev, s) = connect(Config::default()).await;
    let st = s.streamer_state().await.unwrap();
    assert!(st.streamer.is_none() && st.features.h264);
}

#[tokio::test]
async fn wol_list_parses() {
    let (_dev, s) = connect(Config::default()).await;
    assert!(s.wol_list().await.unwrap().is_empty()); // the recorded device has no saved targets
}

#[tokio::test]
async fn unknown_route_is_api_404() {
    let (_dev, s) = connect(Config::default()).await;
    let r: Result<serde_json::Value, _> = s.get("no/such/route", &[]).await;
    assert!(
        matches!(r, Err(Error::Api { status: 404, .. })),
        "{:?}",
        r.err()
    );
}

#[tokio::test]
async fn print_sends_text_body_without_length_limit() {
    let (dev, s) = connect(Config::default()).await;
    let text = "echo héllo\nexit\n";
    let opts = PrintOptions {
        keymap: Some("de"),
        slow: true,
    };
    s.hid_print(text, opts).await.unwrap();
    let call = &dev.calls_to("/api/hid/print")[0];
    assert_eq!(call.body, text);
    for part in ["limit=0", "slow=1", "keymap=de"] {
        assert!(call.query.contains(part), "{}", call.query);
    }
}

#[tokio::test]
async fn two_sessions_stay_valid_together() {
    let (dev, a) = connect(Config::default()).await;
    let b = Session::login(
        &dev.host(),
        &Credentials::new(USER, PASSWORD),
        Arc::new(PinStore::in_memory()),
    )
    .await
    .unwrap();
    a.system_info().await.unwrap();
    b.system_info().await.unwrap();
    assert_eq!(dev.logins(), 2, "a must not have re-logged in");
}

#[tokio::test]
async fn video_format_write_is_read_back() {
    use cometdesk::api::system::VideoFormat;
    let (_dev, s) = connect(Config::default()).await;
    s.set_video_format(VideoFormat::H265).await.unwrap();
    assert_eq!(s.system_config().await.unwrap().video_format, Some(1));
}

#[tokio::test]
async fn streamer_params_write_is_read_back() {
    use cometdesk::api::streamer::StreamerUpdate;
    let (_dev, s) = connect(Config::default()).await;
    let update = StreamerUpdate {
        desired_fps: Some(30),
        ..Default::default()
    };
    let state = s.set_streamer_params(update).await.unwrap();
    assert_eq!(state.params.desired_fps, Some(30));
}

#[tokio::test]
async fn msd_mount_remove_and_upload_round_trip() {
    let (_dev, s) = connect(Config::default()).await;
    s.msd_mount("image-0.iso", true).await.unwrap();
    let msd = s.msd_state().await.unwrap();
    assert!(msd.drive.connected);
    assert_eq!(msd.drive.image.as_deref(), Some("image-0.iso"));
    assert!(s.msd_mount("nope.iso", true).await.is_err());

    // Streamed upload goes out chunked; the mock must see every byte.
    let path = std::env::temp_dir().join(format!("cometdesk-mock-upload-{}", std::process::id()));
    std::fs::write(&path, vec![7u8; 300_000]).unwrap();
    s.msd_upload(&path, "new.iso", |_, _| {}).await.unwrap();
    assert!(s.msd_upload(&path, "new.iso", |_, _| {}).await.is_err());
    std::fs::remove_file(&path).ok();
    let msd = s.msd_state().await.unwrap();
    assert_eq!(msd.storage.images["new.iso"].size, 300_000);
    s.msd_remove("new.iso").await.unwrap();
    assert!(
        !s.msd_state()
            .await
            .unwrap()
            .storage
            .images
            .contains_key("new.iso")
    );
}

#[tokio::test]
async fn wol_add_and_remove_round_trip() {
    use cometdesk::api::wol::Mac;
    let (_dev, s) = connect(Config::default()).await;
    let mac: Mac = "AA:BB:CC:DD:EE:FF".parse().unwrap();
    s.wol_add(mac, "192.168.1.9", "nas").await.unwrap();
    let list = s.wol_list().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "nas");
    s.wol_remove(mac).await.unwrap();
    assert!(s.wol_list().await.unwrap().is_empty());
}

#[tokio::test]
async fn edid_presets_and_switch() {
    let (_dev, s) = connect(Config::default()).await;
    let presets = s.edid_presets().await.unwrap();
    assert!(presets.len() >= 2);
    let other = presets.iter().find(|p| !p.is_default).unwrap();
    s.set_edid(&other.content).await.unwrap();
    assert_eq!(s.edid().await.unwrap(), other.content);
    assert!(s.set_edid("zz").await.is_err());
}

#[tokio::test]
async fn snapshot_needs_a_viewer_unless_offline_allowed() {
    use cometdesk::api::streamer::SnapshotOptions;
    let (_dev, s) = connect(Config::default()).await;
    let idle = SnapshotOptions {
        allow_offline: false,
        preview: None,
    };
    assert!(matches!(
        s.snapshot(idle).await,
        Err(Error::Api { status: 503, .. })
    ));
    let offline = SnapshotOptions {
        allow_offline: true,
        preview: None,
    };
    assert!(
        s.snapshot(offline)
            .await
            .unwrap()
            .starts_with(&[0xff, 0xd8, 0xff])
    );
}

#[tokio::test]
async fn injected_failures_delays_and_drops() {
    let (dev, s) = connect(Config {
        atx: true,
        ..Default::default()
    })
    .await;
    dev.fail_next("/api/atx/power", 503, 1);
    assert!(matches!(
        s.atx_power(PowerAction::On, false).await,
        Err(Error::Api { status: 503, .. })
    ));
    s.atx_power(PowerAction::On, false).await.unwrap();

    dev.set_delay(std::time::Duration::from_millis(300));
    let t = std::time::Instant::now();
    s.system_info().await.unwrap();
    assert!(t.elapsed() >= std::time::Duration::from_millis(300));
    dev.set_delay(std::time::Duration::ZERO);

    dev.drop_next_requests(1);
    assert!(
        s.system_info().await.is_err(),
        "connection closed without an answer"
    );
}

#[tokio::test]
async fn keep_alive_reuses_one_connection() {
    let (dev, s) = connect(Config {
        keep_alive: true,
        ..Default::default()
    })
    .await;
    for _ in 0..5 {
        s.system_info().await.unwrap();
    }
    assert_eq!(
        dev.connections(),
        1,
        "login and all requests share one pooled connection"
    );
}

#[tokio::test]
async fn wrong_method_is_405_and_paths_are_percent_decoded() {
    let (_dev, s) = connect(Config {
        atx: true,
        ..Default::default()
    })
    .await;
    let err = s
        .get::<serde_json::Value>("atx/power", &[])
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Api { status: 405, .. }), "{err:?}");
    let err = s.post::<serde_json::Value>("info", &[]).await.unwrap_err();
    assert!(matches!(err, Error::Api { status: 405, .. }), "{err:?}");
    s.get::<serde_json::Value>("%61tx", &[]).await.unwrap();
}

#[tokio::test]
async fn atx_actions_change_the_power_led() {
    let (_dev, s) = connect(Config {
        atx: true,
        ..Default::default()
    })
    .await;
    let led = |s: &Session| {
        let s = s.clone();
        async move { s.atx_state().await.unwrap().leds.power }
    };
    assert!(led(&s).await);
    s.atx_power(PowerAction::OffHard, false).await.unwrap();
    assert!(!led(&s).await);
    s.atx_power(PowerAction::On, false).await.unwrap();
    assert!(led(&s).await);
    s.atx_power(PowerAction::ResetHard, false).await.unwrap();
    assert!(led(&s).await, "reset keeps the power LED");
}
