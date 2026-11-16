//! Minimal HTTP/1.1 request parsing and reply writing for the mock. Entry points:
//! `read_request`, `write_reply`, `Reply`. Read this to change wire handling.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Decodes `%XX` escapes (invalid ones stay as they are).
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    /// Lowercased names.
    pub headers: Vec<(String, String)>,
    pub body: String,
    /// Bytes in the body (`body` is lossy UTF-8, so its length can differ).
    pub body_len: usize,
    /// The client did not ask to close the connection after this request.
    pub keep_alive: bool,
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

const HEAD_MAX: usize = 64 * 1024;
/// Bytes of a request body kept as text (`Request::body`); the rest is counted, not stored.
const BODY_KEEP: usize = 1 << 20;

struct Body {
    kept: Vec<u8>,
    len: usize,
}

impl Body {
    fn push(&mut self, data: &[u8]) {
        let room = BODY_KEEP.saturating_sub(self.kept.len());
        self.kept.extend_from_slice(&data[..room.min(data.len())]);
        self.len += data.len();
    }
}

/// Reads more into `buf`; `false` when the peer closed.
async fn fill<S: AsyncRead + Unpin>(io: &mut S, buf: &mut Vec<u8>) -> std::io::Result<bool> {
    let mut chunk = [0u8; 16 * 1024];
    let n = io.read(&mut chunk).await?;
    buf.extend_from_slice(&chunk[..n]);
    Ok(n > 0)
}

fn bad(msg: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, msg.to_owned())
}

fn line_end(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

/// Streams a chunked body into `body`, leaving bytes after it in `buf`. `false` on EOF.
async fn read_chunked<S: AsyncRead + Unpin>(
    io: &mut S,
    buf: &mut Vec<u8>,
    body: &mut Body,
) -> std::io::Result<bool> {
    loop {
        let eol = loop {
            if let Some(i) = line_end(buf) {
                break i;
            }
            if buf.len() > 1024 {
                return Err(bad("chunk size line too long"));
            }
            if !fill(io, buf).await? {
                return Ok(false);
            }
        };
        let text = std::str::from_utf8(&buf[..eol]).map_err(|_| bad("chunk size"))?;
        let size = usize::from_str_radix(text.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| bad("chunk size"))?;
        buf.drain(..eol + 2);
        if size == 0 {
            // Optional trailers end with an empty line.
            loop {
                let eol = loop {
                    if let Some(i) = line_end(buf) {
                        break i;
                    }
                    if !fill(io, buf).await? {
                        return Ok(false);
                    }
                };
                buf.drain(..eol + 2);
                if eol == 0 {
                    return Ok(true);
                }
            }
        }
        let mut left = size;
        while left > 0 {
            if buf.is_empty() && !fill(io, buf).await? {
                return Ok(false);
            }
            let t = left.min(buf.len());
            body.push(&buf[..t]);
            buf.drain(..t);
            left -= t;
        }
        while buf.len() < 2 {
            if !fill(io, buf).await? {
                return Ok(false);
            }
        }
        buf.drain(..2);
    }
}

/// `None` when the peer closes before a full request arrives.
/// `buf` carries bytes read past the previous request (pipelining) and is drained of this one.
/// Bodies stream through: only the first `BODY_KEEP` bytes are kept, so uploads stay O(1).
pub async fn read_request<S: AsyncRead + AsyncWrite + Unpin>(
    io: &mut S,
    buf: &mut Vec<u8>,
) -> std::io::Result<Option<Request>> {
    let mut scanned = 0;
    let header_end = loop {
        if let Some(i) = buf[scanned..].windows(4).position(|w| w == b"\r\n\r\n") {
            break scanned + i + 4;
        }
        scanned = buf.len().saturating_sub(3);
        if buf.len() > HEAD_MAX {
            let msg = b"HTTP/1.1 431 X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = io.write_all(msg).await;
            return Ok(None);
        }
        if !fill(io, buf).await? {
            return Ok(None);
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    buf.drain(..header_end);
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
    if headers
        .iter()
        .any(|(k, v)| k == "expect" && v.eq_ignore_ascii_case("100-continue"))
        && buf.is_empty()
    {
        io.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await?;
        io.flush().await?;
    }
    let chunked = headers
        .iter()
        .any(|(k, v)| k == "transfer-encoding" && v.to_ascii_lowercase().contains("chunked"));
    let len: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = Body {
        kept: Vec::new(),
        len: 0,
    };
    if chunked {
        if !read_chunked(io, buf, &mut body).await? {
            return Ok(None);
        }
    } else {
        let mut left = len;
        while left > 0 {
            if buf.is_empty() && !fill(io, buf).await? {
                return Ok(None);
            }
            let t = left.min(buf.len());
            body.push(&buf[..t]);
            buf.drain(..t);
            left -= t;
        }
    }
    let keep_alive = !version.eq_ignore_ascii_case("HTTP/1.0")
        && !headers
            .iter()
            .any(|(k, v)| k == "connection" && v.to_ascii_lowercase().contains("close"));
    // A request line without `HTTP/x` is malformed; `routes::route` answers it with 400.
    let well_formed = version.starts_with("HTTP/") && !method.is_empty() && !target.is_empty();
    Ok(Some(Request {
        method: if well_formed { method } else { "BAD" }.into(),
        path: percent_decode(path),
        query: query.into(),
        headers,
        body: String::from_utf8_lossy(&body.kept).into_owned(),
        body_len: body.len,
        keep_alive,
    }))
}

pub async fn write_reply<S: AsyncWrite + Unpin>(
    io: &mut S,
    r: &Reply,
    head_only: bool,
    keep_alive: bool,
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
        "HTTP/1.1 {} X\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: {}\r\n\r\n",
        r.status,
        payload.len(),
        if keep_alive { "keep-alive" } else { "close" }
    );
    io.write_all(head.as_bytes()).await?;
    if !head_only {
        io.write_all(&payload).await?;
    }
    if keep_alive {
        io.flush().await
    } else {
        io.shutdown().await
    }
}
