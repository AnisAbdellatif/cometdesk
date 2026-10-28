//! Authenticated REST access to one device.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};
use url::Url;

use crate::error::{Error, Result};
use crate::tls::{PinStore, TofuVerifier, client_config};

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Clone)]
pub struct Credentials {
    pub user: String,
    pub password: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("user", &self.user)
            .finish_non_exhaustive()
    }
}

/// An authenticated session with one device. Cheap to clone.
#[derive(Clone)]
pub struct Session {
    inner: Arc<Inner>,
}

struct Inner {
    host: String,
    base: Url,
    http: reqwest::Client,
    tls: Arc<rustls::ClientConfig>,
    token: String,
}

/// kvmd wraps every JSON response as `{"ok": bool, "result": ...}`.
#[derive(Deserialize)]
struct Envelope<T> {
    ok: bool,
    result: T,
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
}

/// The subset of `GET /api/system/get_config` the console uses. Everything is optional:
/// firmware versions differ and the client must not fail on a missing field.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SystemConfig {
    pub keymap: Option<String>,
    pub video_mode: Option<String>,
    pub orientation: Option<i32>,
    pub scroll_rate: Option<u32>,
    pub reverse_scrolling: Option<String>,
    pub relative_sense: Option<u32>,
    pub is_absolute_mouse: Option<bool>,
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

impl Session {
    /// Logs in with user/password. The device certificate is pinned on first contact.
    pub async fn login(host: &str, creds: &Credentials, pins: Arc<PinStore>) -> Result<Session> {
        let host = normalize_host(host);
        let base = Url::parse(&format!("https://{host}/"))?;
        let verifier = Arc::new(TofuVerifier::new(host.clone(), pins));
        let tls = client_config(verifier.clone())?;
        let http = reqwest::Client::builder()
            .tls_backend_preconfigured((*tls).clone())
            .tcp_nodelay(true)
            .http1_only()
            .no_proxy()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()?;

        // URL-encoded like the web app; GL firmware rejects multipart logins.
        let form = [
            ("user", creds.user.as_str()),
            ("passwd", creds.password.as_str()),
        ];
        let resp = http
            .post(base.join("api/auth/login")?)
            .form(&form)
            .send()
            .await
            .map_err(|e| cert_error(&host, &verifier).unwrap_or(Error::Http(e)))?;
        let status = resp.status().as_u16();
        let body = resp.bytes().await?;
        if status != 200 {
            tracing::debug!(status, body = %api_message(&body), "login failed");
        }
        match status {
            200 => {}
            403 => return Err(Error::Forbidden),
            429 => return Err(Error::RateLimited(api_message(&body))),
            _ => {
                return Err(Error::Api {
                    status,
                    message: api_message(&body),
                });
            }
        }
        let login: Envelope<LoginResult> = serde_json::from_slice(&body)?;
        if login.result.two_step_required {
            return Err(Error::TwoStepRequired);
        }
        let token = login
            .result
            .token
            .ok_or_else(|| Error::Protocol("login response has no token".into()))?;

        Ok(Session {
            inner: Arc::new(Inner {
                host,
                base,
                http,
                tls,
                token,
            }),
        })
    }

    pub fn host(&self) -> &str {
        &self.inner.host
    }

    pub fn token(&self) -> &str {
        &self.inner.token
    }

    fn cookie(&self) -> String {
        format!("auth_token={}", self.inner.token)
    }

    /// `GET /api/<path>` and unwrap the kvmd envelope.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let req = self
            .inner
            .http
            .get(self.inner.base.join("api/")?.join(path)?);
        self.send(req).await
    }

    /// `POST /api/<path>?<query>` and unwrap the kvmd envelope.
    pub async fn post<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        let req = self
            .inner
            .http
            .post(self.inner.base.join("api/")?.join(path)?)
            .query(query);
        self.send(req).await
    }

    async fn send<T: DeserializeOwned>(&self, req: reqwest::RequestBuilder) -> Result<T> {
        let resp = req.header("Cookie", self.cookie()).send().await?;
        let status = resp.status().as_u16();
        let body = resp.bytes().await?;
        if status != 200 {
            return Err(Error::Api {
                status,
                message: api_message(&body),
            });
        }
        let env: Envelope<T> = serde_json::from_slice(&body)?;
        if !env.ok {
            return Err(Error::Api {
                status,
                message: api_message(&body),
            });
        }
        Ok(env.result)
    }

    pub async fn system_config(&self) -> Result<SystemConfig> {
        Ok(self.get::<ConfigResult>("system/get_config").await?.config)
    }

    /// Raw `GET /api/streamer` state (shape varies by firmware).
    pub async fn streamer_state(&self) -> Result<serde_json::Value> {
        self.get("streamer").await
    }

    /// Opens a WebSocket at `path_and_query` (e.g. `api/ws?stream=0`) with the session
    /// cookie and the pinned TLS config. Nagle is disabled: HID latency depends on it.
    pub async fn ws_connect(&self, path_and_query: &str) -> Result<WsStream> {
        let url = format!(
            "wss://{}/{}",
            self.inner.host,
            path_and_query.trim_start_matches('/')
        );
        let mut req = url.into_client_request()?;
        let headers = req.headers_mut();
        headers.insert("Cookie", header(&self.cookie())?);
        headers.insert("Origin", header(&format!("https://{}", self.inner.host))?);
        let (ws, _resp) = tokio_tungstenite::connect_async_tls_with_config(
            req,
            None,
            true,
            Some(Connector::Rustls(self.inner.tls.clone())),
        )
        .await?;
        Ok(ws)
    }

    /// Best-effort logout, invalidating the token on the device.
    pub async fn logout(&self) -> Result<()> {
        let _: serde_json::Value = self.post("auth/logout", &[]).await?;
        Ok(())
    }
}

fn header(v: &str) -> Result<HeaderValue> {
    HeaderValue::from_str(v).map_err(|e| Error::Protocol(e.to_string()))
}

fn cert_error(host: &str, verifier: &TofuVerifier) -> Option<Error> {
    verifier
        .take_mismatch()
        .map(|(pinned, presented)| Error::CertChanged {
            host: host.to_owned(),
            pinned,
            presented,
        })
}

fn api_message(body: &[u8]) -> String {
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
    fn parses_system_config_with_missing_and_extra_fields() {
        let json = r#"{"ok":true,"result":{"config":{"keymap":"de","scroll_rate":5,
            "reverse_scrolling":"STANDARD","theme_mode":"1","is_absolute_mouse":true}}}"#;
        let env: Envelope<ConfigResult> = serde_json::from_str(json).unwrap();
        let c = env.result.config;
        assert_eq!(c.keymap.as_deref(), Some("de"));
        assert_eq!(c.scroll_rate, Some(5));
        assert_eq!(c.is_absolute_mouse, Some(true));
        assert_eq!(c.relative_sense, None);
    }

    #[test]
    fn login_result_variants() {
        let ok: Envelope<LoginResult> = serde_json::from_str(
            r#"{"ok":true,"result":{"token":"t","failed_since_last_success":0}}"#,
        )
        .unwrap();
        assert_eq!(ok.result.token.as_deref(), Some("t"));
        let two: Envelope<LoginResult> = serde_json::from_str(
            r#"{"ok":true,"result":{"two_step_required":true,"two_step_token":"x","expires_in":60}}"#,
        )
        .unwrap();
        assert!(two.result.two_step_required);
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
