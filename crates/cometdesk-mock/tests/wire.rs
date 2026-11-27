//! Wire-level tests of the mock over raw TLS: keep-alive, pipelining, HEAD, Expect, limits,
//! idle timeouts, fault injection on the WebSocket upgrade. Read this to add a protocol test.

use std::sync::Arc;
use std::time::{Duration, Instant};

use cometdesk_mock::{Config, MockDevice, PASSWORD};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use tokio_rustls::rustls::crypto::{CryptoProvider, ring};
use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls::{ClientConfig, DigitallySignedStruct, Error, SignatureScheme};

#[derive(Debug)]
struct AcceptAny(Arc<CryptoProvider>);

impl ServerCertVerifier for AcceptAny {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

async fn connect(dev: &MockDevice) -> TlsStream<TcpStream> {
    let provider = Arc::new(ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAny(provider)))
        .with_no_client_auth();
    let tcp = TcpStream::connect(dev.host()).await.unwrap();
    TlsConnector::from(Arc::new(config))
        .connect(ServerName::try_from("localhost").unwrap(), tcp)
        .await
        .unwrap()
}

/// Reads until the server closes or `wait` passes without data. Returns the bytes and whether
/// the server closed.
async fn drain(s: &mut TlsStream<TcpStream>, wait: Duration) -> (String, bool) {
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match timeout(wait, s.read(&mut buf)).await {
            Ok(Ok(0)) | Ok(Err(_)) => return (String::from_utf8_lossy(&out).into_owned(), true),
            Ok(Ok(n)) => out.extend_from_slice(&buf[..n]),
            Err(_) => return (String::from_utf8_lossy(&out).into_owned(), false),
        }
    }
}

/// Status lines (`HTTP/1.1 200 X`); bodies have no trailing newline, so they can abut the next.
fn statuses(raw: &str) -> Vec<&str> {
    raw.match_indices("HTTP/1.1 ")
        .map(|(i, _)| &raw[i..i + 14])
        .collect()
}

async fn login(dev: &MockDevice) -> String {
    let mut s = connect(dev).await;
    let body = format!("user=admin&passwd={}", PASSWORD.replace('$', "%24"));
    let req = format!(
        "POST /api/auth/login HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let (raw, _) = drain(&mut s, Duration::from_secs(2)).await;
    let tok = raw.split("\"token\":\"").nth(1).unwrap();
    format!("auth_token={}", tok.split('"').next().unwrap())
}

fn get(path: &str, cookie: &str, extra: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: x\r\nCookie: {cookie}\r\n{extra}\r\n")
}

#[tokio::test]
async fn pipelined_head_and_close_on_a_keep_alive_connection() {
    let dev = MockDevice::start(Config {
        keep_alive: true,
        ..Default::default()
    })
    .await;
    let ck = login(&dev).await;
    let mut s = connect(&dev).await;
    let wire =
        get("/api/info", &ck, "") + &get("/api/atx", &ck, "") + &get("/api/streamer", &ck, "");
    s.write_all(wire.as_bytes()).await.unwrap();
    let (raw, closed) = drain(&mut s, Duration::from_millis(500)).await;
    assert_eq!(statuses(&raw), ["HTTP/1.1 200 X"; 3], "{raw}");
    assert!(!closed, "keep-alive connection stays open");

    let head = get("/api/atx", &ck, "").replacen("GET", "HEAD", 1);
    s.write_all((head + &get("/api/atx", &ck, "")).as_bytes())
        .await
        .unwrap();
    let (raw, _) = drain(&mut s, Duration::from_millis(500)).await;
    assert_eq!(
        statuses(&raw),
        ["HTTP/1.1 200 X"; 2],
        "HEAD routes like GET: {raw}"
    );
    assert_eq!(raw.matches("{\"ok\"").count(), 1, "HEAD carries no body");

    s.write_all(get("/api/atx", &ck, "Connection: close\r\n").as_bytes())
        .await
        .unwrap();
    let (_, closed) = drain(&mut s, Duration::from_secs(2)).await;
    assert!(closed, "Connection: close is honoured");
}

#[tokio::test]
async fn expect_continue_gets_an_interim_response() {
    let dev = MockDevice::start(Config::default()).await;
    let mut s = connect(&dev).await;
    let body = "user=admin&passwd=nope";
    let head = format!(
        "POST /api/auth/login HTTP/1.1\r\nHost: x\r\nExpect: 100-continue\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    s.write_all(head.as_bytes()).await.unwrap();
    let (raw, _) = drain(&mut s, Duration::from_millis(500)).await;
    assert!(raw.starts_with("HTTP/1.1 100 Continue"), "{raw}");
    s.write_all(body.as_bytes()).await.unwrap();
    let (raw, _) = drain(&mut s, Duration::from_secs(2)).await;
    assert!(raw.contains("HTTP/1.1 403"), "{raw}");
}

#[tokio::test]
async fn oversized_headers_get_431() {
    let dev = MockDevice::start(Config::default()).await;
    let mut s = connect(&dev).await;
    let junk = "a".repeat(100 * 1024);
    let _ = s
        .write_all(format!("GET /x HTTP/1.1\r\nX-Pad: {junk}\r\n").as_bytes())
        .await;
    let (raw, _) = drain(&mut s, Duration::from_secs(2)).await;
    assert!(raw.starts_with("HTTP/1.1 431"), "{raw}");
}

#[tokio::test]
async fn idle_connections_and_stalled_requests_time_out() {
    let dev = MockDevice::start(Config {
        keep_alive: true,
        idle_timeout: Some(Duration::from_millis(300)),
        ..Default::default()
    })
    .await;
    let mut idle = connect(&dev).await;
    let (_, closed) = drain(&mut idle, Duration::from_secs(3)).await;
    assert!(closed, "an idle connection is dropped");
    let mut stalled = connect(&dev).await;
    stalled
        .write_all(b"GET /api/info HTTP/1.1\r\nHost: x\r\n")
        .await
        .unwrap();
    let (_, closed) = drain(&mut stalled, Duration::from_secs(3)).await;
    assert!(closed, "a half-sent request is dropped");
    // A stalled TLS handshake: plain TCP that never speaks.
    let mut raw = TcpStream::connect(dev.host()).await.unwrap();
    let mut b = [0u8; 8];
    let r = timeout(Duration::from_secs(3), raw.read(&mut b))
        .await
        .expect("server must give up");
    assert!(matches!(r, Ok(0) | Err(_)));
}

#[tokio::test]
async fn websocket_upgrade_sees_injected_faults() {
    let dev = MockDevice::start(Config::default()).await;
    let ck = login(&dev).await;
    let upgrade = get(
        "/api/ws?stream=0",
        &ck,
        "Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n",
    );
    dev.fail_next("/api/ws", 503, 1);
    let mut s = connect(&dev).await;
    s.write_all(upgrade.as_bytes()).await.unwrap();
    let (raw, _) = drain(&mut s, Duration::from_secs(1)).await;
    assert!(raw.starts_with("HTTP/1.1 503"), "{raw}");

    dev.set_delay(Duration::from_millis(400));
    let t = Instant::now();
    let mut s = connect(&dev).await;
    s.write_all(upgrade.as_bytes()).await.unwrap();
    let (raw, _) = drain(&mut s, Duration::from_millis(900)).await;
    assert!(raw.starts_with("HTTP/1.1 101"), "{raw}");
    assert!(t.elapsed() >= Duration::from_millis(400));
    dev.set_delay(Duration::ZERO);

    dev.drop_next_requests(1);
    let mut s = connect(&dev).await;
    s.write_all(upgrade.as_bytes()).await.unwrap();
    let (raw, closed) = drain(&mut s, Duration::from_secs(1)).await;
    assert!(raw.is_empty() && closed, "dropped without an answer: {raw}");
}
