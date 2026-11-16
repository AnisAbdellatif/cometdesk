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
