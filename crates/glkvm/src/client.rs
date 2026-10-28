//! Authenticated REST access to one device.
//!
//! Login flow (`POST /api/auth/login`, URL-encoded `user`/`passwd`):
//! - **2FA (TOTP)**: the 6-digit code is appended to the password. Whether it is needed is
//!   public (`GET /api/2fa/is_enabled`), so we check first instead of burning a failed
//!   attempt (the device rate-limits and locks clients).
//! - **Two-step approval**: the device answers with a temporary `two_step_token`; the login
//!   completes once someone approves it on the device, which we poll for with
//!   `POST /api/auth/two_step_complete`.
//!
//! A [`Session`] re-logs in once on HTTP 401 when it was created without a TOTP code (a
//! TOTP code is single-use, so those sessions surface [`Error::Unauthorized`] instead).

use std::sync::{Arc, RwLock};
use std::time::Duration;

use bytes::Bytes;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream, tungstenite};
use url::Url;

use crate::error::{Error, Result};
use crate::tls::{PinStore, TofuVerifier, client_config};

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

const TWO_STEP_POLL: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub struct Credentials {
    pub user: String,
    pub password: String,
    /// Current TOTP code if the device has 2FA enabled.
    pub totp: Option<String>,
}

impl Credentials {
    pub fn new(user: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            user: user.into(),
            password: password.into(),
            totp: None,
        }
    }

    pub fn with_totp(mut self, code: impl Into<String>) -> Self {
        self.totp = Some(code.into());
        self
    }

    /// The `passwd` form value: password, followed by the TOTP code if any.
    fn passwd(&self) -> String {
        match &self.totp {
            Some(code) => format!("{}{}", self.password, code.trim()),
            None => self.password.clone(),
        }
    }
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("user", &self.user)
            .field("totp", &self.totp.is_some())
            .finish_non_exhaustive()
    }
}

/// kvmd wraps every JSON response as `{"ok": bool, "result": ...}`.
#[derive(Deserialize)]
pub(crate) struct Envelope<T> {
    pub(crate) ok: bool,
    pub(crate) result: T,
}

#[derive(Deserialize, Default)]
struct ErrorResult {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_msg: String,
}

#[derive(Deserialize)]
struct LoginResult {
    token: Option<String>,
    #[serde(default)]
    two_step_required: bool,
    two_step_token: Option<String>,
    expires_in: Option<u64>,
    /// `pending` while a two-step login waits for approval.
    status: Option<String>,
}

#[derive(Deserialize)]
struct Enabled {
    enabled: bool,
}

/// The subset of `GET /api/system/get_config` the console uses. Everything is optional:
/// firmware versions differ and the client must not fail on a missing field.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SystemConfig {
    pub keymap: Option<String>,
    pub video_mode: Option<String>,
    pub video_processing: Option<String>,
    pub orientation: Option<i32>,
    pub scroll_rate: Option<u32>,
    pub reverse_scrolling: Option<String>,
    pub relative_sense: Option<u32>,
    pub is_absolute_mouse: Option<bool>,
    pub show_cursor: Option<bool>,
}

#[derive(Deserialize)]
struct ConfigResult {
    config: SystemConfig,
}

/// Normalizes `"host"`, `"host:port"` or `"https://host:port/"` to `host[:port]`.
pub fn normalize_host(input: &str) -> String {
    let s = input.trim();
    let s = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);
    s.trim_end_matches('/').to_owned()
}

/// Unauthenticated transport to one device: HTTP client and TLS pinned to its certificate.
#[derive(Clone)]
pub struct Connection {
    host: String,
    base: Url,
    http: reqwest::Client,
    tls: Arc<rustls::ClientConfig>,
    verifier: Arc<TofuVerifier>,
}

impl Connection {
    pub fn new(host: &str, pins: Arc<PinStore>) -> Result<Self> {
        let host = normalize_host(host);
        let base = Url::parse(&format!("https://{host}/api/"))?;
        let verifier = Arc::new(TofuVerifier::new(host.clone(), pins));
        let tls = client_config(verifier.clone())?;
        let http = reqwest::Client::builder()
            .tls_backend_preconfigured((*tls).clone())
            .tcp_nodelay(true)
            .http1_only()
            .no_proxy()
            .user_agent(concat!("glkvm-rs/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            host,
            base,
            http,
            tls,
            verifier,
        })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    fn url(&self, path: &str) -> Result<Url> {
        Ok(self.base.join(path.trim_start_matches('/'))?)
    }

    /// Maps a transport error, surfacing a changed certificate as [`Error::CertChanged`].
    fn transport(&self, e: reqwest::Error) -> Error {
        match self.verifier.take_mismatch() {
            Some((pinned, presented)) => Error::CertChanged {
                host: self.host.clone(),
                pinned,
                presented,
            },
            None => Error::Http(e),
        }
    }

    /// Whether the device requires a TOTP code at login (no authentication needed).
    pub async fn twofa_enabled(&self) -> Result<bool> {
        let resp = self.http.get(self.url("2fa/is_enabled")?).send().await;
        let (status, body) = read(resp.map_err(|e| self.transport(e))?).await?;
        Ok(parse_envelope::<Enabled>(status, &body)?.enabled)
    }

    /// Starts a login. Returns a session, or a pending two-step login to wait for.
    pub async fn login(&self, creds: &Credentials) -> Result<LoginState> {
        if creds.totp.is_none() && self.twofa_enabled().await? {
            return Err(Error::TotpRequired);
        }
        let passwd = creds.passwd();
        let form = [("user", creds.user.as_str()), ("passwd", passwd.as_str())];
        let resp = self
            .http
            .post(self.url("auth/login")?)
            .form(&form)
            .send()
            .await;
        let (status, body) = read(resp.map_err(|e| self.transport(e))?).await?;
        let login = parse_login(status, &body)?;
        if login.two_step_required {
            let token = login
                .two_step_token
                .ok_or_else(|| Error::Protocol("two-step login without a token".into()))?;
            let expires_in = Duration::from_secs(login.expires_in.unwrap_or(120));
            return Ok(LoginState::AwaitingApproval(Box::new(PendingLogin {
                conn: self.clone(),
                creds: creds.clone(),
                token,
                expires_in,
            })));
        }
        let token = login
            .token
            .ok_or_else(|| Error::Protocol("login without a token".into()))?;
        Ok(LoginState::Done(Session::new(self.clone(), token, creds)))
    }
}

/// Outcome of [`Connection::login`].
pub enum LoginState {
    Done(Session),
    /// Two-step login: someone has to approve this login on the device.
    AwaitingApproval(Box<PendingLogin>),
}

/// A two-step login waiting for approval.
pub struct PendingLogin {
    conn: Connection,
    creds: Credentials,
    token: String,
    expires_in: Duration,
}

impl PendingLogin {
    /// How long the device keeps the request open.
    pub fn expires_in(&self) -> Duration {
        self.expires_in
    }

    /// Checks once. `Ok(None)` while the approval is pending.
    pub async fn poll(&self) -> Result<Option<Session>> {
        let form = [("two_step_token", self.token.as_str())];
        let resp = self
            .conn
            .http
            .post(self.conn.url("auth/two_step_complete")?)
            .form(&form)
            .send()
            .await;
        let (status, body) = read(resp.map_err(|e| self.conn.transport(e))?).await?;
        if status == 403 {
            return Err(Error::TwoStepRejected);
        }
        let result = parse_login(status, &body)?;
        if result.status.as_deref() == Some("pending") {
            return Ok(None);
        }
        let token = result
            .token
            .ok_or_else(|| Error::Protocol("approval without a token".into()))?;
        Ok(Some(Session::new(self.conn.clone(), token, &self.creds)))
    }

    /// Polls every second until approved, rejected or expired.
    pub async fn wait(self) -> Result<Session> {
        let deadline = tokio::time::Instant::now() + self.expires_in;
        loop {
            if let Some(session) = self.poll().await? {
                return Ok(session);
            }
            if tokio::time::Instant::now() + TWO_STEP_POLL > deadline {
                return Err(Error::TwoStepRejected);
            }
            tokio::time::sleep(TWO_STEP_POLL).await;
        }
    }
}

/// An authenticated session with one device. Cheap to clone.
#[derive(Clone)]
pub struct Session {
    inner: Arc<Inner>,
}

struct Inner {
    conn: Connection,
    token: RwLock<String>,
    /// Credentials for re-login on 401; `None` for TOTP logins (codes are single-use).
    relogin: Option<Credentials>,
    relogin_lock: tokio::sync::Mutex<()>,
}

/// Request body for [`Session::request`].
pub(crate) enum Body<'a> {
    None,
    Text(&'a str),
    Bytes(&'a [u8]),
}

impl Session {
    fn new(conn: Connection, token: String, creds: &Credentials) -> Self {
        let relogin = creds.totp.is_none().then(|| creds.clone());
        Session {
            inner: Arc::new(Inner {
                conn,
                token: RwLock::new(token),
                relogin,
                relogin_lock: tokio::sync::Mutex::new(()),
            }),
        }
    }

    /// Logs in, waiting for approval if the device uses two-step login. The device
    /// certificate is pinned on first contact.
    pub async fn login(host: &str, creds: &Credentials, pins: Arc<PinStore>) -> Result<Session> {
        match Connection::new(host, pins)?.login(creds).await? {
            LoginState::Done(session) => Ok(session),
            LoginState::AwaitingApproval(pending) => {
                tracing::info!(timeout = ?pending.expires_in(), "waiting for login approval on the device");
                pending.wait().await
            }
        }
    }

    pub fn host(&self) -> &str {
        &self.inner.conn.host
    }

    pub fn token(&self) -> String {
        self.inner.token.read().unwrap().clone()
    }

    /// Logs in again after the token was rejected. Concurrent callers share one re-login.
    async fn relogin(&self, rejected: &str) -> Result<()> {
        let Some(creds) = &self.inner.relogin else {
            return Err(Error::Unauthorized);
        };
        let _guard = self.inner.relogin_lock.lock().await;
        if self.token() != rejected {
            return Ok(()); // someone else already refreshed it
        }
        tracing::info!("session expired, logging in again");
        match self.inner.conn.login(creds).await? {
            LoginState::Done(fresh) => {
                *self.inner.token.write().unwrap() = fresh.token();
                Ok(())
            }
            LoginState::AwaitingApproval(_) => Err(Error::Unauthorized),
        }
    }

    /// Sends a request, retrying once after a re-login on 401. Returns status and body.
    pub(crate) async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, &str)],
        body: Body<'_>,
    ) -> Result<(u16, Bytes)> {
        let url = self.inner.conn.url(path)?;
        for attempt in 0..2 {
            let token = self.token();
            let mut req = self
                .inner
                .conn
                .http
                .request(method.clone(), url.clone())
                .query(query)
                .header("Cookie", format!("auth_token={token}"));
            match body {
                Body::None => {}
                Body::Text(text) => {
                    req = req
                        .header("Content-Type", "text/plain; charset=utf-8")
                        .body(text.to_owned());
                }
                Body::Bytes(bytes) => {
                    req = req
                        .header("Content-Type", "application/octet-stream")
                        .body(bytes.to_vec());
                }
            }
            let resp = req.send().await.map_err(|e| self.inner.conn.transport(e))?;
            let (status, bytes) = read(resp).await?;
            if status == 401 && attempt == 0 && self.inner.relogin.is_some() {
                self.relogin(&token).await?;
                continue;
            }
            if status == 401 {
                return Err(Error::Unauthorized);
            }
            return Ok((status, bytes));
        }
        unreachable!("loop returns on the second attempt")
    }

    /// `GET /api/<path>?<query>` and unwrap the kvmd envelope.
    pub async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        let (status, body) = self
            .request(reqwest::Method::GET, path, query, Body::None)
            .await?;
        parse_envelope(status, &body)
    }

    /// `POST /api/<path>?<query>` and unwrap the kvmd envelope.
    pub async fn post<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        let (status, body) = self
            .request(reqwest::Method::POST, path, query, Body::None)
            .await?;
        parse_envelope(status, &body)
    }

    pub async fn system_config(&self) -> Result<SystemConfig> {
        Ok(self
            .get::<ConfigResult>("system/get_config", &[])
            .await?
            .config)
    }

    /// Opens a WebSocket at `path_and_query` (e.g. `api/ws?stream=0`) with the session
    /// cookie and the pinned TLS config. Nagle is disabled: HID latency depends on it.
    pub async fn ws_connect(&self, path_and_query: &str) -> Result<WsStream> {
        self.ws_connect_protocol(path_and_query, None).await
    }

    /// Like [`Self::ws_connect`], requesting a WebSocket subprotocol.
    pub async fn ws_connect_protocol(
        &self,
        path_and_query: &str,
        protocol: Option<&str>,
    ) -> Result<WsStream> {
        for attempt in 0..2 {
            let token = self.token();
            match self.ws_connect_once(path_and_query, &token, protocol).await {
                Err(Error::WebSocket(tungstenite::Error::Http(resp)))
                    if resp.status() == 401 && attempt == 0 && self.inner.relogin.is_some() =>
                {
                    self.relogin(&token).await?;
                }
                Err(Error::WebSocket(tungstenite::Error::Http(resp))) if resp.status() == 401 => {
                    return Err(Error::Unauthorized);
                }
                other => return other,
            }
        }
        unreachable!("loop returns on the second attempt")
    }

    async fn ws_connect_once(
        &self,
        path_and_query: &str,
        token: &str,
        protocol: Option<&str>,
    ) -> Result<WsStream> {
        let host = &self.inner.conn.host;
        let url = format!("wss://{host}/{}", path_and_query.trim_start_matches('/'));
        let mut req = url.into_client_request()?;
        let headers = req.headers_mut();
        headers.insert("Cookie", header(&format!("auth_token={token}"))?);
        headers.insert("Origin", header(&format!("https://{host}"))?);
        if let Some(p) = protocol {
            headers.insert("Sec-WebSocket-Protocol", header(p)?);
        }
        let (ws, _resp) = tokio_tungstenite::connect_async_tls_with_config(
            req,
            None,
            true,
            Some(Connector::Rustls(self.inner.conn.tls.clone())),
        )
        .await?;
        Ok(ws)
    }

    /// Invalidates the token on the device.
    pub async fn logout(&self) -> Result<()> {
        let _: serde_json::Value = self.post("auth/logout", &[]).await?;
        Ok(())
    }
}

async fn read(resp: reqwest::Response) -> Result<(u16, Bytes)> {
    let status = resp.status().as_u16();
    Ok((status, resp.bytes().await?))
}

fn header(v: &str) -> Result<HeaderValue> {
    HeaderValue::from_str(v).map_err(|e| Error::Protocol(e.to_string()))
}

/// Unwraps `{"ok": true, "result": T}`, mapping errors to [`Error`].
pub(crate) fn parse_envelope<T: DeserializeOwned>(status: u16, body: &[u8]) -> Result<T> {
    if status != 200 {
        return Err(status_error(status, body));
    }
    let env: Envelope<T> = serde_json::from_slice(body)?;
    if !env.ok {
        return Err(Error::Api {
            status,
            message: api_message(body),
        });
    }
    Ok(env.result)
}

fn parse_login(status: u16, body: &[u8]) -> Result<LoginResult> {
    match status {
        403 => Err(Error::Forbidden),
        429 => Err(Error::RateLimited(api_message(body))),
        _ => parse_envelope(status, body),
    }
}

pub(crate) fn status_error(status: u16, body: &[u8]) -> Error {
    match status {
        401 => Error::Unauthorized,
        _ => Error::Api {
            status,
            message: api_message(body),
        },
    }
}

pub(crate) fn api_message(body: &[u8]) -> String {
    match serde_json::from_slice::<Envelope<ErrorResult>>(body) {
        Ok(env) if !env.result.error_msg.is_empty() => env.result.error_msg,
        Ok(env) if !env.result.error.is_empty() => env.result.error,
        _ => String::from_utf8_lossy(&body[..body.len().min(200)]).into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_normalization() {
        assert_eq!(normalize_host("192.168.8.174"), "192.168.8.174");
        assert_eq!(
            normalize_host(" https://kvm.local:8443/ "),
            "kvm.local:8443"
        );
        assert_eq!(normalize_host("http://kvm"), "kvm");
    }

    #[test]
    fn totp_code_is_appended_to_the_password() {
        assert_eq!(Credentials::new("admin", "pw").passwd(), "pw");
        assert_eq!(
            Credentials::new("admin", "pw")
                .with_totp(" 123456 ")
                .passwd(),
            "pw123456"
        );
    }

    #[test]
    fn credentials_debug_hides_secrets() {
        let s = format!(
            "{:?}",
            Credentials::new("admin", "hunter2").with_totp("123456")
        );
        assert!(!s.contains("hunter2") && !s.contains("123456"), "{s}");
    }

    #[test]
    fn parses_system_config_with_missing_and_extra_fields() {
        let json = r#"{"ok":true,"result":{"config":{"keymap":"de","scroll_rate":5,
            "reverse_scrolling":"STANDARD","theme_mode":"1","is_absolute_mouse":true}}}"#;
        let c = parse_envelope::<ConfigResult>(200, json.as_bytes())
            .unwrap()
            .config;
        assert_eq!(c.keymap.as_deref(), Some("de"));
        assert_eq!(c.scroll_rate, Some(5));
        assert_eq!(c.is_absolute_mouse, Some(true));
        assert_eq!(c.relative_sense, None);
    }

    #[test]
    fn login_responses() {
        let ok = parse_login(
            200,
            br#"{"ok":true,"result":{"token":"t","failed_since_last_success":0}}"#,
        )
        .unwrap();
        assert_eq!(ok.token.as_deref(), Some("t"));
        let two = parse_login(
            200,
            br#"{"ok":true,"result":{"two_step_required":true,"two_step_token":"x","expires_in":60}}"#,
        )
        .unwrap();
        assert!(two.two_step_required);
        assert_eq!(
            (two.two_step_token.as_deref(), two.expires_in),
            (Some("x"), Some(60))
        );
        let pending = parse_login(200, br#"{"ok":true,"result":{"status":"pending"}}"#).unwrap();
        assert_eq!(pending.status.as_deref(), Some("pending"));
        assert!(matches!(
            parse_login(403, br#"{"ok":false,"result":{"error":"ForbiddenError"}}"#),
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            parse_login(429, br#"{"ok":false,"result":{"error":"RateLimitError","error_msg":"wait"}}"#),
            Err(Error::RateLimited(m)) if m == "wait"
        ));
    }

    #[test]
    fn envelope_errors() {
        assert!(matches!(
            parse_envelope::<serde_json::Value>(401, b""),
            Err(Error::Unauthorized)
        ));
        let unavailable = br#"{"ok":false,"result":{"error":"UnavailableError","error_msg":"Service Unavailable"}}"#;
        assert!(matches!(
            parse_envelope::<serde_json::Value>(503, unavailable),
            Err(Error::Api { status: 503, message }) if message == "Service Unavailable"
        ));
        assert!(matches!(
            parse_envelope::<serde_json::Value>(200, br#"{"ok":false,"result":{}}"#),
            Err(Error::Api { status: 200, .. })
        ));
    }

    #[test]
    fn api_error_message_extraction() {
        let body = br#"{"ok":false,"result":{"error":"RateLimitError","error_msg":"slow down"}}"#;
        assert_eq!(api_message(body), "slow down");
        assert_eq!(
            api_message(b"<html>bad gateway</html>"),
            "<html>bad gateway</html>"
        );
    }
}
