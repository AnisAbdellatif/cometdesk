//! Authenticated `Session` methods: login, HTTP/JSON/binary requests, 401 re-login, and WebSockets.
//! Entry points: `Session::login`, `Session::begin_login`, `Session::get`, `Session::post`, `Session::ws`.
//! To change pre-login connection setup or 2FA/two-step polling, read `mod.rs`.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use bytes::Bytes;
use serde::de::DeserializeOwned;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{Connector, tungstenite};

use super::{
    Body, ConfigResult, Connection, Credentials, Inner, LoginState, Session, SystemConfig,
    WS_CONNECT_TIMEOUT, WsStream, header, parse_envelope, read,
};
use crate::error::{Error, Result};
use crate::tls::PinStore;

impl Session {
    pub(super) fn new(conn: Connection, token: String, creds: &Credentials) -> Self {
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

    /// `POST /api/<path>?<query>` with a streamed body and no overall timeout beyond a day.
    /// Not retried: a stream can't be replayed, so a rejected token fails the call.
    pub(crate) async fn post_stream<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
        body: reqwest::Body,
    ) -> Result<T> {
        let resp = self
            .inner
            .conn
            .http
            .post(self.inner.conn.url(path)?)
            .query(query)
            .timeout(Duration::from_secs(24 * 3600))
            .header("Cookie", format!("auth_token={}", self.token()))
            .header("Content-Type", "application/octet-stream")
            .body(body)
            .send()
            .await
            .map_err(|e| self.inner.conn.transport(e))?;
        let (status, bytes) = read(resp).await?;
        if status == 401 {
            return Err(Error::Unauthorized);
        }
        parse_envelope(status, &bytes)
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
                Body::Json(json) => {
                    req = req
                        .header("Content-Type", "application/json")
                        .body(json.to_owned());
                }
                Body::Bytes(bytes) => {
                    req = req
                        .header("Content-Type", "application/octet-stream")
                        .body(bytes.to_vec());
                }
                Body::Field(name, value) => {
                    const BOUNDARY: &str = "----cometdesk-field-boundary";
                    req = req
                        .header(
                            "Content-Type",
                            format!("multipart/form-data; boundary={BOUNDARY}"),
                        )
                        .body(format!(
                            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n--{BOUNDARY}--\r\n"
                        ));
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
        let connect = tokio_tungstenite::connect_async_tls_with_config(
            req,
            None,
            true,
            Some(Connector::Rustls(self.inner.conn.tls.clone())),
        );
        let (ws, _resp) = tokio::time::timeout(WS_CONNECT_TIMEOUT, connect)
            .await
            .map_err(|_| Error::Protocol(format!("websocket connect to {host} timed out")))??;
        Ok(ws)
    }

    /// Invalidates the token on the device.
    pub async fn logout(&self) -> Result<()> {
        let _: serde_json::Value = self.post("auth/logout", &[]).await?;
        Ok(())
    }
}
