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
    /// Bytes in the body (`body` is lossy UTF-8, so its length can differ).
    pub body_len: usize,
}

/// Status plus the JSON that goes inside the `{"ok":..,"result":..}` envelope.
pub struct Reply {
    pub status: u16,
    pub json: String,
    /// Answer `json` as is, without the `{"ok":..,"result":..}` envelope.
    pub bare: bool,
    /// Binary payload and its content type; overrides `json`.
    pub raw: Option<(&'static str, Vec<u8>)>,
}

impl Reply {
    pub fn ok(json: impl Into<String>) -> Reply {
        Reply {
            status: 200,
            json: json.into(),
            bare: false,
            raw: None,
        }
    }

    /// A 200 whose JSON is not wrapped in the envelope (`upgrade/edid_list`).
    pub fn bare(json: impl Into<String>) -> Reply {
        Reply {
            bare: true,
            ..Reply::ok(json)
        }
    }

    pub fn jpeg(bytes: &[u8]) -> Reply {
        Reply {
            raw: Some(("image/jpeg", bytes.to_vec())),
            ..Reply::ok("")
        }
    }

    /// kvmd error shape: `{"ok":false,"result":{"error":..,"error_msg":..}}`.
    pub fn error(status: u16, error: &str) -> Reply {
        let msg = error.trim_end_matches("Error");
        Reply {
            status,
            json: format!(r#"{{"error":"{error}","error_msg":"{msg}"}}"#),
            bare: false,
            raw: None,
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
    let (method, target, version) = (
        first.next().unwrap_or_default(),
        first.next().unwrap_or_default(),
        first.next().unwrap_or_default(),
    );
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let chunked = headers
        .iter()
        .any(|(k, v)| k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked"));
    let len: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let body_bytes = if chunked {
        loop {
            if let Some(b) = decode_chunked(&buf[header_end..]) {
                break b;
            }
            let n = io.read(&mut chunk).await?;
            if n == 0 {
                return Ok(None);
            }
            buf.extend_from_slice(&chunk[..n]);
        }
    } else {
        while buf.len() < header_end + len {
            let n = io.read(&mut chunk).await?;
            if n == 0 {
                return Ok(None);
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        buf[header_end..header_end + len].to_vec()
    };
    // A request line without `HTTP/x` is malformed; `routes::route` answers it with 400.
    let well_formed = version.starts_with("HTTP/") && !method.is_empty() && !target.is_empty();
    Ok(Some(Request {
        method: if well_formed { method } else { "BAD" }.into(),
        path: path.into(),
        query: query.into(),
        headers,
        body: String::from_utf8_lossy(&body_bytes).to_string(),
        body_len: body_bytes.len(),
    }))
}

/// The body of a complete chunked message, or `None` while the terminating chunk is missing.
pub(crate) fn decode_chunked(mut data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let eol = data.windows(2).position(|w| w == b"\r\n")?;
        let size_text = std::str::from_utf8(&data[..eol]).ok()?;
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        data = &data[eol + 2..];
        if size == 0 {
            // Optional trailers end with an empty line.
            return (data.starts_with(b"\r\n") || data.windows(4).any(|w| w == b"\r\n\r\n"))
                .then_some(out);
        }
        if data.len() < size + 2 {
            return None;
        }
        out.extend_from_slice(&data[..size]);
        data = &data[size + 2..];
    }
}

pub async fn write_reply<S: AsyncWrite + Unpin>(
    io: &mut S,
    r: &Reply,
    head_only: bool,
) -> std::io::Result<()> {
    let (ctype, payload) = match &r.raw {
        Some((t, b)) => (*t, b.clone()),
        None if r.bare => ("application/json", r.json.clone().into_bytes()),
        None => (
            "application/json",
            format!(r#"{{"ok":{},"result":{}}}"#, r.status == 200, r.json).into_bytes(),
        ),
    };
    let head = format!(
        "HTTP/1.1 {} X\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        r.status,
        payload.len()
    );
    io.write_all(head.as_bytes()).await?;
    if !head_only {
        io.write_all(&payload).await?;
    }
    io.shutdown().await
}
