//! A root shell on the device through its web terminal (`/extras/webterm/ttyd/ws`, ttyd).
//!
//! The firmware runs ttyd as root behind the same nginx login check as the API, so this needs
//! nothing beyond the normal session: no SSH, no extra credentials. It is what the web UI's own
//! terminal uses; anyone with the admin login already has it. Used to install and repair our
//! device-side components (`bridge/`).
//!
//! ttyd protocol: the first client message is JSON (`AuthToken`, terminal size); afterwards
//! input is `'0' + bytes` and output arrives as `'0' + bytes`. Commands run in an interactive
//! bash on a pty, so each command line must stay below the tty line limit (4096 bytes).

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use crate::client::WsStream;
use crate::{Error, Result, Session};

const PATH: &str = "extras/webterm/ttyd/ws";
/// Longest command line we send (the pty's canonical line buffer is 4096 bytes).
pub const MAX_LINE: usize = 3800;
const UPLOAD_CHUNK: usize = 3000;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

pub struct RootShell {
    ws: WsStream,
    out: String,
    seq: u64,
    tag: String,
}

/// Output and exit status of one command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub status: i32,
    pub text: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.status == 0
    }
}

impl Session {
    /// Opens a root shell on the device through its web terminal.
    pub async fn root_shell(&self) -> Result<RootShell> {
        let ws = self.ws_connect_protocol(PATH, Some("tty")).await?;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let mut sh = RootShell {
            ws,
            out: String::new(),
            seq: 0,
            tag: format!("{nanos:08x}"),
        };
        let init = r#"{"AuthToken":"","columns":250,"rows":50}"#;
        sh.ws
            .send(Message::Binary(init.as_bytes().to_vec().into()))
            .await?;
        // No echo, no prompt noise: only command output and our end markers remain.
        sh.run("stty -echo; export PS1= PS2=").await?;
        Ok(sh)
    }
}

impl RootShell {
    /// Runs `cmd` and waits for it, with the default timeout.
    pub async fn run(&mut self, cmd: &str) -> Result<Output> {
        self.run_timeout(cmd, DEFAULT_TIMEOUT).await
    }

    pub async fn run_timeout(&mut self, cmd: &str, timeout: Duration) -> Result<Output> {
        if cmd.len() > MAX_LINE || cmd.contains('\n') {
            return Err(Error::Protocol(
                "shell command too long or multi-line".into(),
            ));
        }
        self.seq += 1;
        // The marker's digits come from `$?`; an echoed command line shows a literal `$?`,
        // so it can never be mistaken for the end of the output.
        let marker = format!("__GLKVM_{}_{}_", self.tag, self.seq);
        let input = format!("{cmd}\rprintf '\\n{marker}%d__\\n' $?\r");
        let mut msg = Vec::with_capacity(input.len() + 1);
        msg.push(b'0');
        msg.extend_from_slice(input.as_bytes());
        self.ws.send(Message::Binary(msg.into())).await?;
        tokio::time::timeout(timeout, self.read_until(&marker))
            .await
            .map_err(|_| Error::Protocol(format!("shell command timed out: {cmd:.60}")))?
    }

    async fn read_until(&mut self, marker: &str) -> Result<Output> {
        loop {
            if let Some((start, status, consumed)) = find_end(&self.out, marker) {
                let text = self.out[..start].replace('\r', "");
                // The previous end marker's newline, and the one our marker starts with.
                let text = text.strip_prefix('\n').unwrap_or(&text);
                let text = text.strip_suffix('\n').unwrap_or(text).to_owned();
                self.out.drain(..consumed);
                return Ok(Output { status, text });
            }
            match self.ws.next().await {
                Some(Ok(Message::Binary(b))) if b.first() == Some(&b'0') => {
                    self.out.push_str(&String::from_utf8_lossy(&b[1..]));
                }
                Some(Ok(Message::Close(_))) | None => {
                    return Err(Error::Protocol("device closed the shell".into()));
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => return Err(e.into()),
            }
        }
    }

    /// Writes `data` to `path` on the device (atomically, with `mode`, e.g. `0755`) and
    /// checks its SHA-256.
    pub async fn upload(&mut self, path: &str, data: &[u8], mode: &str) -> Result<()> {
        check_path(path)?;
        let tmp = format!("{path}.glkvm-part");
        self.ok(&format!(
            "mkdir -p \"$(dirname '{path}')\" && : > '{tmp}.b64'"
        ))
        .await?;
        let b64 = base64(data);
        for chunk in b64.as_bytes().chunks(UPLOAD_CHUNK) {
            let chunk = std::str::from_utf8(chunk).expect("base64 is ASCII");
            self.ok(&format!("printf '%s' '{chunk}' >> '{tmp}.b64'"))
                .await?;
        }
        self.ok(&format!(
            "base64 -d '{tmp}.b64' > '{tmp}' && rm '{tmp}.b64' && chmod {mode} '{tmp}'"
        ))
        .await?;
        let sum = self.ok(&format!("sha256sum '{tmp}'")).await?;
        let want = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(data));
        if !sum.text.starts_with(&want) {
            let _ = self.run(&format!("rm -f '{tmp}'")).await;
            return Err(Error::Protocol(format!("upload of {path} corrupted")));
        }
        self.ok(&format!("mv -f '{tmp}' '{path}'")).await?;
        Ok(())
    }

    /// Runs `cmd`, failing unless it exits with 0.
    pub async fn ok(&mut self, cmd: &str) -> Result<Output> {
        let out = self.run(cmd).await?;
        if !out.success() {
            return Err(Error::Protocol(format!(
                "`{cmd:.80}` failed ({}): {}",
                out.status,
                out.text.trim()
            )));
        }
        Ok(out)
    }

    pub async fn close(mut self) {
        let _ = self
            .ws
            .send(Message::Binary(b"0exit\r".to_vec().into()))
            .await;
        let _ = self.ws.close(None).await;
    }
}

/// Finds `<marker><digits>__` in `out`: returns where the marker starts, the exit status and
/// how many bytes to consume. Occurrences followed by anything but digits (the echoed
/// command line shows `%d`) are skipped.
fn find_end(out: &str, marker: &str) -> Option<(usize, i32, usize)> {
    out.match_indices(marker).find_map(|(start, _)| {
        let rest = &out[start + marker.len()..];
        let end = rest.find("__")?;
        let digits = &rest[..end];
        (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then(|| {
            (
                start,
                digits.parse().unwrap_or(-1),
                start + marker.len() + end + 2,
            )
        })
    })
}

fn check_path(path: &str) -> Result<()> {
    if path.starts_with('/')
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
    {
        Ok(())
    } else {
        Err(Error::Protocol(format!("unsafe device path {path:?}")))
    }
}

fn base64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            A[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            A[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        for (input, want) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), want);
        }
    }

    #[test]
    fn end_marker_skips_the_echoed_command() {
        let m = "__GLKVM_ab_1_";
        let out = format!("ls\r\nprintf '\\n{m}%d__\\n' $?\r\nfile.txt\r\n\r\n{m}0__\r\nrest");
        let (start, status, consumed) = find_end(&out, m).unwrap();
        assert_eq!(status, 0);
        assert!(out[..start].ends_with("file.txt\r\n\r\n"));
        assert_eq!(&out[consumed..], "\r\nrest");
        assert_eq!(find_end(&format!("{m}%d__ only echo"), m), None);
        assert_eq!(find_end(&format!("{m}127__"), m).map(|r| r.1), Some(127));
    }

    #[test]
    fn paths_are_restricted() {
        assert!(check_path("/usr/local/lib/glkvm-bridge/x.py").is_ok());
        assert!(check_path("relative").is_err());
        assert!(check_path("/tmp/a'b").is_err());
        assert!(check_path("/tmp/$(x)").is_err());
    }
}
