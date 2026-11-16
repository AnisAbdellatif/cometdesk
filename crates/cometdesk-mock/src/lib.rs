//! Test GLKVM device: TLS + REST + `/api/ws`, answering from recorded fixtures.
//! Entry point: `MockDevice::start`. Files: `http` (wire), `routes` (REST), `ws` (events/HID),
//! `fixtures` (embedded recordings). Read `routes.rs` to add an endpoint.

mod device;
mod faults;
mod fixtures;
mod http;
mod routes;
mod stateful;
mod ws;

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::{self, pki_types::PrivateKeyDer};

pub const USER: &str = "admin";
pub const PASSWORD: &str = "pa$$word";
pub const TOTP: &str = "123456";

#[derive(Default, Clone)]
pub struct Config {
    pub twofa: bool,
    pub two_step: bool,
    /// Pending answers before a two-step login is approved.
    pub two_step_pending: u32,
    /// Report an ATX board and accept power actions.
    pub atx: bool,
    /// Serve several requests per connection (HTTP keep-alive) instead of closing after one.
    pub keep_alive: bool,
}

/// One authenticated REST request.
#[derive(Debug, Clone)]
pub struct Call {
    pub method: String,
    pub path: String,
    pub query: String,
    pub body: String,
}

#[derive(Clone)]
pub(crate) enum Push {
    Text(String),
    Close,
    /// Close the sockets that logged in with this token (logout).
    CloseToken(String),
}

pub(crate) struct State {
    pub config: Config,
    pub acceptor: Mutex<TlsAcceptor>,
    pub tokens: Mutex<HashSet<String>>,
    pub dev: Mutex<device::Device>,
    pub faults: Mutex<faults::Faults>,
    pub connections: AtomicU32,
    pub logins: AtomicU32,
    pub polls: AtomicU32,
    pub issued: AtomicU32,
    pub calls: Mutex<Vec<Call>>,
    pub hid_frames: Mutex<Vec<Vec<u8>>>,
    pub ws_sessions: AtomicU32,
    pub viewers: AtomicU32,
    pub push: broadcast::Sender<Push>,
}

impl State {
    pub fn issue_token(&self) -> String {
        let t = format!("token-{}", self.issued.fetch_add(1, Ordering::SeqCst));
        self.tokens.lock().unwrap().insert(t.clone());
        t
    }

    pub fn authed(&self, req: &http::Request) -> bool {
        req.token()
            .is_some_and(|t| self.tokens.lock().unwrap().contains(t))
    }
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

pub struct MockDevice {
    addr: SocketAddr,
    state: Arc<State>,
}

impl MockDevice {
    /// Binds `127.0.0.1:0` and serves until the runtime ends.
    pub async fn start(config: Config) -> MockDevice {
        Self::start_on(config, "127.0.0.1:0").await
    }

    /// Like `start`, but binds `bind` (e.g. `127.0.0.1:8443`).
    pub async fn start_on(config: Config, bind: &str) -> MockDevice {
        let state = Arc::new(State {
            config,
            acceptor: Mutex::new(acceptor()),
            tokens: Mutex::new(HashSet::new()),
            dev: Mutex::new(device::Device::new()),
            faults: Mutex::new(faults::Faults::default()),
            connections: AtomicU32::new(0),
            logins: AtomicU32::new(0),
            polls: AtomicU32::new(0),
            issued: AtomicU32::new(0),
            calls: Mutex::new(Vec::new()),
            hid_frames: Mutex::new(Vec::new()),
            ws_sessions: AtomicU32::new(0),
            viewers: AtomicU32::new(0),
            push: broadcast::channel(64).0,
        });
        let listener = TcpListener::bind(bind).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let st = state.clone();
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                let acceptor = st.acceptor.lock().unwrap().clone();
                let st = st.clone();
                st.connections.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    if let Ok(mut tls) = acceptor.accept(tcp).await {
                        let _ = handle(st, &mut tls).await;
                    }
                });
            }
        });
        MockDevice { addr, state }
    }

    /// `127.0.0.1:PORT`.
    pub fn host(&self) -> String {
        self.addr.to_string()
    }

    /// Simulates a factory reset / new certificate.
    pub fn rotate_cert(&self) {
        *self.state.acceptor.lock().unwrap() = acceptor();
    }

    pub fn expire_token(&self) {
        self.state.tokens.lock().unwrap().clear();
    }

    pub fn logins(&self) -> u32 {
        self.state.logins.load(Ordering::SeqCst)
    }

    /// `two_step_complete` calls so far.
    pub fn polls(&self) -> u32 {
        self.state.polls.load(Ordering::SeqCst)
    }

    /// Authenticated REST requests in arrival order.
    pub fn calls(&self) -> Vec<Call> {
        self.state.calls.lock().unwrap().clone()
    }

    pub fn calls_to(&self, path: &str) -> Vec<Call> {
        self.calls()
            .into_iter()
            .filter(|c| c.path == path)
            .collect()
    }

    /// Binary `/api/ws` frames received.
    pub fn hid_frames(&self) -> Vec<Vec<u8>> {
        self.state.hid_frames.lock().unwrap().clone()
    }

    /// Successful `/api/ws` upgrades.
    pub fn ws_sessions(&self) -> u32 {
        self.state.ws_sessions.load(Ordering::SeqCst)
    }

    /// Open `/api/ws` sockets without `stream=0`.
    pub fn viewers(&self) -> u32 {
        self.state.viewers.load(Ordering::SeqCst)
    }

    /// Sends `{"event_type":kind,"event":data}` to every open socket.
    pub fn push_event(&self, kind: &str, data: serde_json::Value) {
        let text = serde_json::json!({"event_type": kind, "event": data}).to_string();
        let _ = self.state.push.send(Push::Text(text));
    }

    /// Pushes `kickout`, then closes every socket.
    pub fn kickout(&self) {
        self.push_event("kickout", serde_json::json!({}));
        self.drop_sockets();
    }

    /// TCP connections accepted so far (shows whether keep-alive reuses them).
    pub fn connections(&self) -> u32 {
        self.state.connections.load(Ordering::SeqCst)
    }

    /// Delays every REST answer by `d`.
    pub fn set_delay(&self, d: std::time::Duration) {
        self.state.faults.lock().unwrap().delay = d;
    }

    /// Answers the next `times` requests to `path` (e.g. `/api/atx/power`) with `status`.
    pub fn fail_next(&self, path: &str, status: u16, times: u32) {
        self.state
            .faults
            .lock()
            .unwrap()
            .fail
            .push((path.into(), status, times));
    }

    /// Closes the next `n` REST connections without answering.
    pub fn drop_next_requests(&self, n: u32) {
        self.state.faults.lock().unwrap().drop_next = n;
    }

    /// Closes every `/api/ws` socket.
    pub fn drop_sockets(&self) {
        let _ = self.state.push.send(Push::Close);
    }
}

async fn handle<S>(state: Arc<State>, io: &mut S) -> std::io::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut buf = Vec::new();
    loop {
        let Some(req) = http::read_request(io, &mut buf).await? else {
            return Ok(());
        };
        let upgrade = req
            .header("upgrade")
            .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
        let keep = state.config.keep_alive && req.keep_alive;
        if req.path == "/api/ws" && upgrade {
            if !state.authed(&req) {
                let r = http::Reply::error(401, "UnauthorizedError");
                return http::write_reply(io, &r, false, false).await;
            }
            ws::serve(state, io, &req).await;
            return Ok(());
        }
        let reply = match faults::apply(&state, &req).await {
            faults::Fault::Drop => return Ok(()),
            faults::Fault::Reply(r) => r,
            faults::Fault::None => routes::route(&state, &req),
        };
        http::write_reply(io, &reply, req.method == "HEAD", keep).await?;
        if !keep {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests;
