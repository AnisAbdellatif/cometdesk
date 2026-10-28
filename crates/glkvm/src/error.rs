use tokio_tungstenite::tungstenite;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("websocket: {0}")]
    WebSocket(#[from] tungstenite::Error),
    #[error("invalid url: {0}")]
    Url(#[from] url::ParseError),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("tls: {0}")]
    Tls(#[from] rustls::Error),
    #[error("login rejected: wrong user or password")]
    Forbidden,
    #[error("login rate-limited by the device: {0}")]
    RateLimited(String),
    #[error("the device requires two-step login approval, which is not supported yet")]
    TwoStepRequired,
    #[error("device returned HTTP {status}: {message}")]
    Api { status: u16, message: String },
    #[error(
        "certificate for {host} changed (pinned {pinned}, presented {presented}); \
         remove the entry from the known-hosts file if this is expected"
    )]
    CertChanged {
        host: String,
        pinned: String,
        presented: String,
    },
    #[error("protocol: {0}")]
    Protocol(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
