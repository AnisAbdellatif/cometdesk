//! Login, re-login and certificate pinning against a mock kvmd over real TLS.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cometdesk::api::hid::PrintOptions;
use cometdesk::tls::PinStore;
use cometdesk::{Connection, Credentials, Error, LoginState, Session};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::{self, pki_types::PrivateKeyDer};

const PASSWORD: &str = "pa$$word";

#[derive(Default)]
struct Config {
    twofa: bool,
    two_step: bool,
    /// Pending answers before a two-step login is approved.
    two_step_pending: u32,
}

struct Mock {
    config: Config,
    acceptor: Mutex<TlsAcceptor>,
    valid_token: Mutex<Option<String>>,
    logins: AtomicU32,
    polls: AtomicU32,
    issued: AtomicU32,
    printed: Mutex<Vec<(String, String)>>,
}

fn acceptor() -> TlsAcceptor {
    let ck = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let key = PrivateKeyDer::Pkcs8(ck.signing_key.serialize_der().into());
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![ck.cert.der().clone()], key)
    .unwrap();
    TlsAcceptor::from(Arc::new(config))
}

impl Mock {
    async fn start(config: Config) -> (Arc<Mock>, SocketAddr) {
        let mock = Arc::new(Mock {
            config,
            acceptor: Mutex::new(acceptor()),
            valid_token: Mutex::new(None),
            logins: AtomicU32::new(0),
            polls: AtomicU32::new(0),
            issued: AtomicU32::new(0),
            printed: Mutex::new(Vec::new()),
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let m = mock.clone();
        tokio::spawn(async move {
            loop {
                let (tcp, _) = listener.accept().await.unwrap();
                let acceptor = m.acceptor.lock().unwrap().clone();
                let m = m.clone();
                tokio::spawn(async move {
                    if let Ok(mut tls) = acceptor.accept(tcp).await {
                        let _ = m.handle(&mut tls).await;
                    }
                });
            }
        });
        (mock, addr)
    }

    /// Simulates a factory reset / new certificate.
    fn rotate_cert(&self) {
        *self.acceptor.lock().unwrap() = acceptor();
    }

    fn expire_token(&self) {
        *self.valid_token.lock().unwrap() = None;
    }

    fn issue_token(&self) -> String {
        let t = format!("token-{}", self.issued.fetch_add(1, Ordering::SeqCst));
        *self.valid_token.lock().unwrap() = Some(t.clone());
        t
    }

    async fn handle<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
        &self,
        io: &mut S,
    ) -> std::io::Result<()> {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        let header_end = loop {
            let n = io.read(&mut chunk).await?;
            if n == 0 {
                return Ok(());
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
        let len: usize = head
            .lines()
            .find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse().unwrap())
            })
            .unwrap_or(0);
        while buf.len() < header_end + len {
            let n = io.read(&mut chunk).await?;
            buf.extend_from_slice(&chunk[..n]);
        }
        let body = String::from_utf8_lossy(&buf[header_end..header_end + len]).to_string();
        let mut parts = head.lines().next().unwrap().split(' ');
        let (method, target) = (parts.next().unwrap(), parts.next().unwrap());
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let token = head.lines().find_map(|l| {
            l.strip_prefix("Cookie: auth_token=")
                .or_else(|| l.strip_prefix("cookie: auth_token="))
        });
        let authed =
            token.is_some() && token.map(str::to_owned) == *self.valid_token.lock().unwrap();
        let form = |s: &str| -> Vec<(String, String)> {
            url::form_urlencoded::parse(s.as_bytes())
                .into_owned()
                .collect()
        };
        let field = |pairs: &[(String, String)], k: &str| {
            pairs
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };

        let (status, json) = match (method, path) {
            ("GET", "/api/2fa/is_enabled") => {
                (200, format!(r#"{{"enabled":{}}}"#, self.config.twofa))
            }
            ("POST", "/api/auth/login") => {
                self.logins.fetch_add(1, Ordering::SeqCst);
                let f = form(&body);
                let want = if self.config.twofa {
                    format!("{PASSWORD}123456")
                } else {
                    PASSWORD.into()
                };
                if field(&f, "user") != "admin" || field(&f, "passwd") != want {
                    (
                        403,
                        r#"{"error":"ForbiddenError","error_msg":"Forbidden"}"#.into(),
                    )
                } else if self.config.two_step {
                    (
                        200,
                        r#"{"two_step_required":true,"two_step_token":"tst","expires_in":30}"#
                            .into(),
                    )
                } else {
                    (200, format!(r#"{{"token":"{}"}}"#, self.issue_token()))
                }
            }
            ("POST", "/api/auth/two_step_complete") => {
                if field(&form(&body), "two_step_token") != "tst" {
                    (403, r#"{"error":"ForbiddenError"}"#.into())
                } else if self.polls.fetch_add(1, Ordering::SeqCst) < self.config.two_step_pending {
                    (200, r#"{"status":"pending"}"#.into())
                } else {
                    (200, format!(r#"{{"token":"{}"}}"#, self.issue_token()))
                }
            }
            (_, _) if !authed => (
                401,
                r#"{"error":"UnauthorizedError","error_msg":"Unauthorized"}"#.into(),
            ),
            ("GET", "/api/atx") => (
                200,
                r#"{"enabled":true,"busy":false,"leds":{"power":true,"hdd":false},"power":"on"}"#
                    .into(),
            ),
            ("POST", "/api/hid/print") => {
                self.printed
                    .lock()
                    .unwrap()
                    .push((query.to_owned(), body.clone()));
                (200, "{}".into())
            }
            _ => (404, r#"{"error":"NotFoundError"}"#.into()),
        };
        let ok = status == 200;
        let payload = format!(r#"{{"ok":{ok},"result":{json}}}"#);
        let resp = format!(
            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        );
        io.write_all(resp.as_bytes()).await?;
        io.shutdown().await
    }
}

fn creds() -> Credentials {
    Credentials::new("admin", PASSWORD)
}

fn pins() -> Arc<PinStore> {
    Arc::new(PinStore::in_memory())
}

#[tokio::test]
async fn plain_login_and_authenticated_request() {
    let (mock, addr) = Mock::start(Config::default()).await;
    let session = Session::login(&addr.to_string(), &creds(), pins())
        .await
        .unwrap();
    let atx = session.atx_state().await.unwrap();
    assert!(atx.enabled && atx.leds.power);
    assert_eq!(mock.logins.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn wrong_password_is_forbidden() {
    let (_mock, addr) = Mock::start(Config::default()).await;
    let bad = Credentials::new("admin", "nope");
    assert!(matches!(
        Session::login(&addr.to_string(), &bad, pins()).await,
        Err(Error::Forbidden)
    ));
}

#[tokio::test]
async fn totp_required_is_detected_without_a_login_attempt() {
    let (mock, addr) = Mock::start(Config {
        twofa: true,
        ..Default::default()
    })
    .await;
    let r = Session::login(&addr.to_string(), &creds(), pins()).await;
    assert!(matches!(r, Err(Error::TotpRequired)), "{:?}", r.err());
    assert_eq!(
        mock.logins.load(Ordering::SeqCst),
        0,
        "no failed attempt counted"
    );
}

#[tokio::test]
async fn totp_code_is_appended_to_the_password() {
    let (_mock, addr) = Mock::start(Config {
        twofa: true,
        ..Default::default()
    })
    .await;
    let session = Session::login(&addr.to_string(), &creds().with_totp("123456"), pins()).await;
    assert!(session.is_ok(), "{:?}", session.err());
}

#[tokio::test]
async fn two_step_login_waits_for_approval() {
    let config = Config {
        two_step: true,
        two_step_pending: 2,
        ..Default::default()
    };
    let (mock, addr) = Mock::start(config).await;
    let conn = Connection::new(&addr.to_string(), pins()).unwrap();
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
    assert_eq!(mock.polls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn expired_token_triggers_one_relogin() {
    let (mock, addr) = Mock::start(Config::default()).await;
    let session = Session::login(&addr.to_string(), &creds(), pins())
        .await
        .unwrap();
    let first = session.token();
    mock.expire_token();
    session.atx_state().await.expect("re-login and retry");
    assert_ne!(session.token(), first);
    assert_eq!(mock.logins.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn concurrent_requests_share_one_relogin() {
    let (mock, addr) = Mock::start(Config::default()).await;
    let session = Session::login(&addr.to_string(), &creds(), pins())
        .await
        .unwrap();
    mock.expire_token();
    let (a, b, c) = tokio::join!(
        session.atx_state(),
        session.atx_state(),
        session.atx_state()
    );
    assert!(a.is_ok() && b.is_ok() && c.is_ok());
    assert_eq!(
        mock.logins.load(Ordering::SeqCst),
        2,
        "one initial + one shared re-login"
    );
}

#[tokio::test]
async fn totp_sessions_do_not_relogin() {
    let (mock, addr) = Mock::start(Config {
        twofa: true,
        ..Default::default()
    })
    .await;
    let session = Session::login(&addr.to_string(), &creds().with_totp("123456"), pins())
        .await
        .unwrap();
    mock.expire_token();
    assert!(matches!(
        session.atx_state().await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(mock.logins.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn changed_certificate_is_refused() {
    let (mock, addr) = Mock::start(Config::default()).await;
    let pins = pins();
    Session::login(&addr.to_string(), &creds(), pins.clone())
        .await
        .unwrap();
    mock.rotate_cert();
    let r = Session::login(&addr.to_string(), &creds(), pins).await;
    match r {
        Err(Error::CertChanged {
            host,
            pinned,
            presented,
        }) => {
            assert_eq!(host, addr.to_string());
            assert_ne!(pinned, presented);
        }
        other => panic!("expected CertChanged, got {:?}", other.err()),
    }
}

#[tokio::test]
async fn print_sends_text_body_without_length_limit() {
    let (mock, addr) = Mock::start(Config::default()).await;
    let session = Session::login(&addr.to_string(), &creds(), pins())
        .await
        .unwrap();
    let text = "echo héllo\nexit\n";
    session
        .hid_print(
            text,
            PrintOptions {
                keymap: Some("de"),
                slow: true,
            },
        )
        .await
        .unwrap();
    let printed = mock.printed.lock().unwrap();
    let (query, body) = &printed[0];
    assert_eq!(body, text);
    for part in ["limit=0", "slow=1", "keymap=de"] {
        assert!(query.contains(part), "{query}");
    }
}
