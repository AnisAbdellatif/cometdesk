//! Login, re-login and certificate pinning against the `cometdesk-mock` device over real TLS.

use std::sync::Arc;
use std::time::Duration;

use cometdesk::tls::PinStore;
use cometdesk::{Connection, Credentials, Error, LoginState, Session};
use cometdesk_mock::{Config, MockDevice, PASSWORD, TOTP};

fn creds() -> Credentials {
    Credentials::new("admin", PASSWORD)
}

fn pins() -> Arc<PinStore> {
    Arc::new(PinStore::in_memory())
}

#[tokio::test]
async fn plain_login_and_authenticated_request() {
    let mock = MockDevice::start(Config {
        atx: true,
        ..Default::default()
    })
    .await;
    let session = Session::login(&mock.host(), &creds(), pins())
        .await
        .unwrap();
    let atx = session.atx_state().await.unwrap();
    assert!(atx.enabled && atx.leds.power);
    assert_eq!(mock.logins(), 1);
}

#[tokio::test]
async fn wrong_password_is_forbidden() {
    let mock = MockDevice::start(Config::default()).await;
    let bad = Credentials::new("admin", "nope");
    assert!(matches!(
        Session::login(&mock.host(), &bad, pins()).await,
        Err(Error::Forbidden)
    ));
}

#[tokio::test]
async fn totp_required_is_detected_without_a_login_attempt() {
    let mock = MockDevice::start(Config {
        twofa: true,
        ..Default::default()
    })
    .await;
    let r = Session::login(&mock.host(), &creds(), pins()).await;
    assert!(matches!(r, Err(Error::TotpRequired)), "{:?}", r.err());
    assert_eq!(mock.logins(), 0, "no failed attempt counted");
}

#[tokio::test]
async fn totp_code_is_appended_to_the_password() {
    let mock = MockDevice::start(Config {
        twofa: true,
        ..Default::default()
    })
    .await;
    let session = Session::login(&mock.host(), &creds().with_totp(TOTP), pins()).await;
    assert!(session.is_ok(), "{:?}", session.err());
}

#[tokio::test]
async fn two_step_login_waits_for_approval() {
    let config = Config {
        two_step: true,
        two_step_pending: 2,
        ..Default::default()
    };
    let mock = MockDevice::start(config).await;
    let conn = Connection::new(&mock.host(), pins()).unwrap();
    let LoginState::AwaitingApproval(pending) = conn.login(&creds()).await.unwrap() else {
        panic!("expected a pending two-step login");
    };
    assert_eq!(pending.expires_in(), Duration::from_secs(30));
    assert!(
        pending.poll().await.unwrap().is_none(),
        "first poll is pending"
    );
    let session = pending.wait().await.unwrap();
    assert!(session.atx_state().await.is_ok());
    assert_eq!(mock.polls(), 3);
}

#[tokio::test]
async fn expired_token_triggers_one_relogin() {
    let mock = MockDevice::start(Config::default()).await;
    let session = Session::login(&mock.host(), &creds(), pins())
        .await
        .unwrap();
    let first = session.token();
    mock.expire_token();
    session.atx_state().await.expect("re-login and retry");
    assert_ne!(session.token(), first);
    assert_eq!(mock.logins(), 2);
}

#[tokio::test]
async fn concurrent_requests_share_one_relogin() {
    let mock = MockDevice::start(Config::default()).await;
    let session = Session::login(&mock.host(), &creds(), pins())
        .await
        .unwrap();
    mock.expire_token();
    let (a, b, c) = tokio::join!(
        session.atx_state(),
        session.atx_state(),
        session.atx_state()
    );
    assert!(a.is_ok() && b.is_ok() && c.is_ok());
    assert_eq!(mock.logins(), 2, "one initial + one shared re-login");
}

#[tokio::test]
async fn totp_sessions_do_not_relogin() {
    let mock = MockDevice::start(Config {
        twofa: true,
        ..Default::default()
    })
    .await;
    let session = Session::login(&mock.host(), &creds().with_totp(TOTP), pins())
        .await
        .unwrap();
    mock.expire_token();
    assert!(matches!(
        session.atx_state().await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(mock.logins(), 1);
}

#[tokio::test]
async fn changed_certificate_is_refused() {
    let mock = MockDevice::start(Config::default()).await;
    let pins = pins();
    Session::login(&mock.host(), &creds(), pins.clone())
        .await
        .unwrap();
    mock.rotate_cert();
    let r = Session::login(&mock.host(), &creds(), pins).await;
    match r {
        Err(Error::CertChanged {
            host,
            pinned,
            presented,
        }) => {
            assert_eq!(host, mock.host());
            assert_ne!(pinned, presented);
        }
        other => panic!("expected CertChanged, got {:?}", other.err()),
    }
}
