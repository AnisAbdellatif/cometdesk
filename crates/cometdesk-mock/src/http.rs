//! Minimal HTTP/1.1 request parsing and reply writing for the mock. Entry points:
//! `read_request`, `write_reply`, `Reply`. Read this to change wire handling.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    /// Lowercased names.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Status plus the JSON that goes inside the `{"ok":..,"result":..}` envelope.
pub struct Reply {
    pub status: u16,
    pub json: String,
}

impl Reply {
    pub fn ok(json: impl Into<String>) -> Reply {
        Reply {
            status: 200,
            json: json.into(),
        }
    }

    /// kvmd error shape: `{"ok":false,"result":{"error":..,"error_msg":..}}`.
    pub fn error(status: u16, error: &str) -> Reply {
        let msg = error.trim_end_matches("Error");
        Reply {
            status,
            json: format!(r#"{{"error":"{error}","error_msg":"{msg}"}}"#),
        }
    }
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The `auth_token` cookie.
    pub fn token(&self) -> Option<&str> {
        self.header("cookie")?
            .split(';')
            .find_map(|c| c.trim().strip_prefix("auth_token="))
    }
}

/// `None` when the peer closes before a full request arrives.
pub async fn read_request<S: AsyncRead + Unpin>(io: &mut S) -> std::io::Result<Option<Request>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        let n = io.read(&mut chunk).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.lines();
    let mut first = lines.next().unwrap_or_default().split(' ');
    let (method, target) = (
        first.next().unwrap_or_default(),
        first.next().unwrap_or_default(),
    );
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let len: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    while buf.len() < header_end + len {
        let n = io.read(&mut chunk).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = String::from_utf8_lossy(&buf[header_end..header_end + len]).to_string();
    Ok(Some(Request {
        method: method.into(),
        path: path.into(),
        query: query.into(),
        headers,
        body,
    }))
}

pub async fn write_reply<S: AsyncWrite + Unpin>(io: &mut S, r: &Reply) -> std::io::Result<()> {
    let payload = format!(r#"{{"ok":{},"result":{}}}"#, r.status == 200, r.json);
    let resp = format!(
        "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        r.status,
        payload.len()
    );
    io.write_all(resp.as_bytes()).await?;
    io.shutdown().await
}
