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
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use url::Url;

use crate::error::{Error, Result};
use crate::tls::{PinStore, TofuVerifier, client_config};

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

const TWO_STEP_POLL: Duration = Duration::from_secs(1);
/// WebSocket connect timeout. Unlike the REST client, `tokio_tungstenite` has none of its
/// own, so a black-holed host would otherwise hang the connect at OS-timeout scale.
const WS_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

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
    pub venc_mode: Option<String>,
    /// 0 = H.264, 1 = H.265.
    pub video_format: Option<i64>,
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
            .user_agent(concat!("cometdesk-rs/", env!("CARGO_PKG_VERSION")))
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
    Json(&'a str),
    Bytes(&'a [u8]),
    /// `multipart/form-data` with one text field, `(name, value)`.
    Field(&'a str, &'a str),
}

mod session;

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
mod tests;
