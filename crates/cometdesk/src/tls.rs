//! Trust-on-first-use (TOFU) pinning of the device certificate.
//!
//! GLKVM devices ship a self-signed certificate, so CA validation is useless. Instead we pin
//! the SHA-256 of the end-entity certificate on first contact and refuse a different one
//! later. The TLS handshake signature is still verified, so a pinned certificate proves the
//! peer holds the matching private key.

use std::collections::BTreeMap;
use std::fmt;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};

/// Host → certificate fingerprint store, optionally persisted as `host sha256hex` lines.
#[derive(Debug)]
pub struct PinStore {
    path: Option<PathBuf>,
    pins: Mutex<BTreeMap<String, String>>,
}

impl PinStore {
    /// Store that only lives in memory (tests, one-off tools).
    pub fn in_memory() -> Self {
        Self {
            path: None,
            pins: Mutex::default(),
        }
    }

    /// Store persisted at `<config dir>/cometdesk/known_hosts` (see [`config_dir`]).
    pub fn load_default() -> io::Result<Self> {
        let dir = app_dir().ok_or_else(|| io::Error::other("no config directory"))?;
        Self::load(dir.join("known_hosts"))
    }

    pub fn load(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let pins = match std::fs::read_to_string(&path) {
            Ok(text) => parse_pins(&text),
            Err(e) if e.kind() == io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(e),
        };
        Ok(Self {
            path: Some(path),
            pins: Mutex::new(pins),
        })
    }

    pub fn get(&self, host: &str) -> Option<String> {
        self.pins.lock().unwrap().get(host).cloned()
    }

    pub fn pin(&self, host: &str, fingerprint: &str) -> io::Result<()> {
        self.remember(host, fingerprint);
        self.flush()
    }

    /// Records a pin in memory only. Persist with [`Self::flush`].
    pub(crate) fn remember(&self, host: &str, fingerprint: &str) {
        self.pins
            .lock()
            .unwrap()
            .insert(host.to_owned(), fingerprint.to_owned());
    }

    /// Writes the current pins to disk, if this store is persisted. Blocking (does an
    /// `fsync`): never call it directly from an async task — see [`TofuVerifier`].
    pub(crate) fn flush(&self) -> io::Result<()> {
        if let Some(path) = &self.path {
            let pins = self.pins.lock().unwrap();
            save_pins(path, &pins)?;
        }
        Ok(())
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

/// Per-user configuration directory: `~/Library/Application Support` on macOS,
/// `%APPDATA%` on Windows, `$XDG_CONFIG_HOME` or `~/.config` elsewhere.
pub fn config_dir() -> Option<PathBuf> {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support"))
    } else if cfg!(windows) {
        env("APPDATA")
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))
    }
}

/// The app's directory inside [`config_dir`]. A directory left by the app's former name
/// (`glkvm`) is renamed into place the first time, so saved devices and pins carry over.
pub fn app_dir() -> Option<PathBuf> {
    let base = config_dir()?;
    let dir = base.join("cometdesk");
    let old = base.join("glkvm");
    if !dir.exists() && old.is_dir() {
        let _ = std::fs::rename(&old, &dir);
    }
    Some(dir)
}

fn parse_pins(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (host, fp) = l.split_once(char::is_whitespace)?;
            Some((host.to_owned(), fp.trim().to_ascii_lowercase()))
        })
        .collect()
}

fn save_pins(path: &Path, pins: &BTreeMap<String, String>) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::File::create(&tmp)?;
    writeln!(
        f,
        "# cometdesk pinned device certificates: <host> <sha256 of DER cert>"
    )?;
    for (host, fp) in pins {
        writeln!(f, "{host} {fp}")?;
    }
    f.sync_all()?;
    std::fs::rename(tmp, path)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinCheck {
    /// Matches the pinned fingerprint.
    Match,
    /// First contact: the fingerprint was pinned now.
    Pinned,
    /// A different certificate is pinned for this host.
    Mismatch { pinned: String },
}

/// Pure pin decision; persists on first contact.
pub fn check_pin(store: &PinStore, host: &str, fingerprint: &str) -> io::Result<PinCheck> {
    match store.get(host) {
        Some(p) if p == fingerprint => Ok(PinCheck::Match),
        Some(pinned) => Ok(PinCheck::Mismatch { pinned }),
        None => {
            store.pin(host, fingerprint)?;
            Ok(PinCheck::Pinned)
        }
    }
}

pub fn fingerprint(cert: &[u8]) -> String {
    hex::encode(Sha256::digest(cert))
}

/// rustls verifier implementing TOFU for one host.
pub struct TofuVerifier {
    host: String,
    store: Arc<PinStore>,
    provider: Arc<CryptoProvider>,
    mismatch: Mutex<Option<(String, String)>>,
}

impl fmt::Debug for TofuVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TofuVerifier")
            .field("host", &self.host)
            .finish()
    }
}

impl TofuVerifier {
    pub fn new(host: impl Into<String>, store: Arc<PinStore>) -> Self {
        Self {
            host: host.into(),
            store,
            provider: Arc::new(rustls::crypto::ring::default_provider()),
            mismatch: Mutex::default(),
        }
    }

    /// `(pinned, presented)` if the certificate presented for this host no longer matches the
    /// pin. Does not consume the record: every concurrent request that fails on a changed
    /// certificate reports it, and a later successful handshake clears it.
    pub fn take_mismatch(&self) -> Option<(String, String)> {
        self.mismatch.lock().unwrap().clone()
    }

    /// Persists the pin store off the async worker. The pin is already in memory, so the
    /// handshake does not wait for the disk.
    fn flush_store(&self) {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let store = self.store.clone();
                handle.spawn_blocking(move || {
                    if let Err(e) = store.flush() {
                        tracing::warn!(%e, "could not persist the pinned certificate");
                    }
                });
            }
            Err(_) => {
                let _ = self.store.flush();
            }
        }
    }
}

impl ServerCertVerifier for TofuVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        // The disk write on first contact is kept off this thread: this verifier runs inside
        // the TLS handshake on an async worker, and `save_pins` does a blocking `fsync`.
        let fp = fingerprint(end_entity);
        match self.store.get(&self.host) {
            Some(pinned) if pinned == fp => {
                *self.mismatch.lock().unwrap() = None;
                Ok(ServerCertVerified::assertion())
            }
            Some(pinned) => {
                *self.mismatch.lock().unwrap() = Some((pinned, fp));
                Err(rustls::Error::General("device certificate changed".into()))
            }
            None => {
                self.store.remember(&self.host, &fp);
                self.flush_store();
                *self.mismatch.lock().unwrap() = None;
                tracing::info!(host = %self.host, fingerprint = %fp, "pinned device certificate");
                Ok(ServerCertVerified::assertion())
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// One rustls config shared by the REST client and all WebSockets of a session.
/// No ALPN: kvmd's nginx speaks HTTP/1.1, and WebSockets need it anyway.
pub fn client_config(verifier: Arc<TofuVerifier>) -> Result<Arc<ClientConfig>, rustls::Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_contact_pins_then_matches() {
        let store = PinStore::in_memory();
        assert_eq!(
            check_pin(&store, "kvm:443", "aa").unwrap(),
            PinCheck::Pinned
        );
        assert_eq!(check_pin(&store, "kvm:443", "aa").unwrap(), PinCheck::Match);
    }

    #[test]
    fn changed_cert_is_rejected() {
        let store = PinStore::in_memory();
        check_pin(&store, "kvm", "aa").unwrap();
        assert_eq!(
            check_pin(&store, "kvm", "bb").unwrap(),
            PinCheck::Mismatch {
                pinned: "aa".into()
            }
        );
        // Not overwritten by the rejected cert.
        assert_eq!(store.get("kvm").as_deref(), Some("aa"));
    }

    #[test]
    fn pins_are_per_host() {
        let store = PinStore::in_memory();
        check_pin(&store, "a", "aa").unwrap();
        assert_eq!(check_pin(&store, "b", "bb").unwrap(), PinCheck::Pinned);
    }

    #[test]
    fn persists_and_reloads() {
        let dir = std::env::temp_dir().join(format!("cometdesk-pin-test-{}", std::process::id()));
        let path = dir.join("known_hosts");
        let _ = std::fs::remove_dir_all(&dir);
        PinStore::load(&path)
            .unwrap()
            .pin("10.0.0.1", "ABCDEF")
            .unwrap();
        let reloaded = PinStore::load(&path).unwrap();
        // Stored as given, compared lowercase after reload.
        assert_eq!(reloaded.get("10.0.0.1").as_deref(), Some("abcdef"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parse_ignores_comments_and_blank_lines() {
        let pins = parse_pins("# c\n\nhost1 aa\n  host2   BB  \nbroken\n");
        assert_eq!(pins.len(), 2);
        assert_eq!(pins["host2"], "bb");
    }

    #[test]
    fn fingerprint_is_sha256_hex() {
        assert_eq!(
            fingerprint(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
