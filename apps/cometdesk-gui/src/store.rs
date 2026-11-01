//! Saved devices (`<config dir>/cometdesk/devices.json`) and their passwords (OS keychain).
//!
//! Passwords never touch the JSON file. Auth tokens are kept in memory only.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use cometdesk_view::KeyOptions;
use serde::{Deserialize, Serialize};

// Kept from the pre-rename name so saved passwords stay readable.
const KEYCHAIN_SERVICE: &str = "cometdesk";
/// Keychain service of the app's former name; items found there are moved to the new one.
const LEGACY_KEYCHAIN_SERVICE: &str = "glkvm";

/// What the target runs; decides modifier mapping and the type-clipboard shortcut.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetOs {
    #[default]
    Windows,
    Linux,
    MacOs,
    /// Firmware setup screens and boot loaders: no modifier translation.
    Bios,
}

impl TargetOs {
    pub const ALL: [TargetOs; 4] = [
        TargetOs::Windows,
        TargetOs::Linux,
        TargetOs::MacOs,
        TargetOs::Bios,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TargetOs::Windows => "Windows",
            TargetOs::Linux => "Linux",
            TargetOs::MacOs => "macOS",
            TargetOs::Bios => "BIOS / other",
        }
    }

    /// Mac client → Windows/Linux target: Cmd acts as Ctrl so Cmd+C/V/Z behave as expected.
    pub fn key_options(self, iso_fix: bool) -> KeyOptions {
        KeyOptions {
            iso_fix,
            swap_cmd_ctrl: matches!(self, TargetOs::Windows | TargetOs::Linux),
        }
    }
}

/// Which way the clipboard bridge syncs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipDirection {
    #[default]
    Both,
    /// This Mac's clipboard goes to the target; the target's never comes back.
    ToTarget,
    /// The target's clipboard comes to this Mac; ours is never sent.
    FromTarget,
}

impl ClipDirection {
    pub fn sends(self) -> bool {
        self != ClipDirection::FromTarget
    }

    pub fn receives(self) -> bool {
        self != ClipDirection::ToTarget
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedDevice {
    pub name: String,
    /// `host` or `host:port`.
    pub host: String,
    pub user: String,
    pub target: TargetOs,
    /// Keyboard layout of the target (device keymap name, e.g. `de`), used to type text.
    /// `None`: the device's default.
    pub keymap: Option<String>,
    /// Clipboard bridge direction. The pairing secret itself is in the keychain.
    pub clip_direction: ClipDirection,
    /// Paste keystroke for "Insert clipboard" on Linux targets.
    pub insert_paste: cometdesk_clip::PastePref,
    /// Video source (`direct`, `webrtc-fec`, `webrtc`); empty: Direct.
    pub transport: String,
    /// Play the target's audio (WebRTC FEC source only).
    pub audio: bool,
    /// Output device name for the target's audio; empty: the system default.
    pub audio_output: String,
}

impl SavedDevice {
    fn keychain_account(&self) -> String {
        format!("{}@{}", self.user, self.host)
    }

    fn legacy_clip_keychain_account(&self) -> String {
        format!("clipboard-bridge:{}@{}", self.user, self.host)
    }

    pub fn display_name(&self) -> &str {
        if self.name.trim().is_empty() {
            &self.host
        } else {
            &self.name
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct File {
    devices: Vec<SavedDevice>,
}

pub struct Store {
    path: Option<PathBuf>,
    pub devices: Vec<SavedDevice>,
}

impl Store {
    pub fn load_default() -> Self {
        let path = cometdesk::tls::app_dir().map(|d| d.join("devices.json"));
        match &path {
            Some(p) => Self::load(p).unwrap_or_else(|e| {
                tracing::warn!(path = %p.display(), %e, "cannot read saved devices");
                Self {
                    path: path.clone(),
                    devices: Vec::new(),
                }
            }),
            None => Self {
                path: None,
                devices: Vec::new(),
            },
        }
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let devices = match std::fs::read(path) {
            Ok(bytes) => {
                serde_json::from_slice::<File>(&bytes)
                    .map_err(io::Error::other)?
                    .devices
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        Ok(Self {
            path: Some(path.to_owned()),
            devices,
        })
    }

    pub fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(&File {
            devices: self.devices.clone(),
        })
        .map_err(io::Error::other)?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, path)
    }

    /// Adds or replaces (same host and user) a device.
    pub fn upsert(&mut self, device: SavedDevice) {
        match self
            .devices
            .iter_mut()
            .find(|d| d.host == device.host && d.user == device.user)
        {
            Some(existing) => *existing = device,
            None => self.devices.push(device),
        }
    }

    pub fn remove(&mut self, index: usize) -> Option<SavedDevice> {
        (index < self.devices.len()).then(|| self.devices.remove(index))
    }
}

/// Everything kept in the keychain for one device. One item per device means one access prompt
/// per device (macOS asks per item), not one for the password and another for the pairing secret.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Credentials {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    /// Pairing secret of the clipboard agent on the target, as `Secret::to_code`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    clip: Option<String>,
}

/// Marks the JSON form; an item without it is a bare password from before the merge.
const BLOB_PREFIX: &str = "cometdesk:v1:";
const LEGACY_BLOB_PREFIX: &str = "glkvm:v1:";

impl Credentials {
    fn parse(raw: &str) -> Credentials {
        match raw
            .strip_prefix(BLOB_PREFIX)
            .or_else(|| raw.strip_prefix(LEGACY_BLOB_PREFIX))
        {
            Some(json) => serde_json::from_str(json).unwrap_or_default(),
            None => Credentials {
                password: Some(raw.to_owned()),
                clip: None,
            },
        }
    }

    fn encode(&self) -> String {
        format!(
            "{BLOB_PREFIX}{}",
            serde_json::to_string(self).unwrap_or_default()
        )
    }
}

/// Keychain reads per account, so one process prompts at most once per device however many
/// times the password and the pairing secret are asked for. Only successful reads are cached:
/// a denied prompt must never be mistaken for "nothing stored" and overwrite the item.
static CACHE: Mutex<BTreeMap<String, Credentials>> = Mutex::new(BTreeMap::new());

fn entry(account: &str) -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, account)
}

fn legacy_entry(account: &str) -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(LEGACY_KEYCHAIN_SERVICE, account)
}

/// How long one successful Touch ID check covers further keychain reads.
const UNLOCK_WINDOW: Duration = Duration::from_secs(300);
static UNLOCKED_AT: Mutex<Option<Instant>> = Mutex::new(None);

/// Touch ID (or password) before the keychain is read. A refusal is an error, never "no entry",
/// so it can't lead to an item being overwritten.
fn unlock() -> keyring::Result<()> {
    let mut at = UNLOCKED_AT.lock().unwrap();
    if at.is_some_and(|t| t.elapsed() < UNLOCK_WINDOW) {
        return Ok(());
    }
    crate::touchid::authenticate("unlock your saved device passwords")
        .map_err(|e| keyring::Error::NoStorageAccess(e.into()))?;
    *at = Some(Instant::now());
    Ok(())
}

/// Debug builds keep credentials in a plain file instead of the keychain, so restarting during
/// development never prompts for Touch ID or the keychain password. Never compiled into release.
#[cfg(debug_assertions)]
mod dev {
    use std::collections::BTreeMap;

    fn path() -> Option<std::path::PathBuf> {
        cometdesk::tls::app_dir().map(|d| d.join("dev-credentials.json"))
    }

    fn read() -> BTreeMap<String, String> {
        path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn get(account: &str) -> Option<String> {
        read().remove(account)
    }

    pub fn set(account: &str, raw: Option<&str>) -> Result<(), String> {
        let mut map = read();
        match raw {
            Some(r) => map.insert(account.to_owned(), r.to_owned()),
            None => map.remove(account),
        };
        let p = path().ok_or("no config dir")?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(&map).map_err(|e| e.to_string())?;
        std::fs::write(&p, json).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

fn load(device: &SavedDevice) -> keyring::Result<Credentials> {
    let account = device.keychain_account();
    #[cfg(debug_assertions)]
    return Ok(dev::get(&account)
        .map(|raw| Credentials::parse(&raw))
        .unwrap_or_default());
    #[cfg(not(debug_assertions))]
    load_keychain(device, account)
}

#[cfg_attr(debug_assertions, allow(dead_code))]
fn load_keychain(device: &SavedDevice, account: String) -> keyring::Result<Credentials> {
    if let Some(c) = CACHE.lock().unwrap().get(&account) {
        return Ok(c.clone());
    }
    unlock()?;
    tracing::debug!(%account, "keychain read");
    let mut creds = match entry(&account)?.get_password() {
        Ok(raw) => Credentials::parse(&raw),
        Err(keyring::Error::NoEntry) => Credentials::default(),
        Err(e) => return Err(e),
    };
    // Item written under the former service name: move it over, then drop the old one.
    if creds == Credentials::default()
        && let Ok(old) = legacy_entry(&account)
        && let Ok(raw) = old.get_password()
    {
        creds = Credentials::parse(&raw);
        if save(device, &creds).is_ok() {
            let _ = old.delete_credential();
        }
    }
    // Pairing secret stored in its own item by earlier versions: fold it in.
    if creds.clip.is_none()
        && let Ok(legacy) = entry(&device.legacy_clip_keychain_account())
        && let Ok(code) = legacy.get_password()
    {
        creds.clip = Some(code);
        if save(device, &creds).is_ok() {
            let _ = legacy.delete_credential();
        }
    }
    CACHE.lock().unwrap().insert(account, creds.clone());
    Ok(creds)
}

fn save(device: &SavedDevice, creds: &Credentials) -> keyring::Result<()> {
    let account = device.keychain_account();
    #[cfg(debug_assertions)]
    return dev::set(
        &account,
        (*creds != Credentials::default())
            .then(|| creds.encode())
            .as_deref(),
    )
    .map_err(|e| keyring::Error::NoStorageAccess(e.into()));
    #[cfg(not(debug_assertions))]
    save_keychain(&account, creds)
}

#[cfg_attr(debug_assertions, allow(dead_code))]
fn save_keychain(account: &str, creds: &Credentials) -> keyring::Result<()> {
    let account = account.to_owned();
    tracing::debug!(%account, "keychain write");
    if *creds == Credentials::default() {
        match entry(&account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(e),
        }
    } else {
        entry(&account)?.set_password(&creds.encode())?;
    }
    CACHE.lock().unwrap().insert(account, creds.clone());
    Ok(())
}

fn update(device: &SavedDevice, f: impl FnOnce(&mut Credentials)) -> Result<(), String> {
    let mut creds = load(device).map_err(|e| e.to_string())?;
    f(&mut creds);
    save(device, &creds).map_err(|e| e.to_string())
}

pub fn password(device: &SavedDevice) -> Option<String> {
    load(device).ok()?.password
}

pub fn set_password(device: &SavedDevice, password: &str) -> Result<(), String> {
    update(device, |c| c.password = Some(password.to_owned()))
}

/// Removes the device's password and pairing secret.
pub fn delete_password(device: &SavedDevice) {
    let _ = save(device, &Credentials::default());
    if cfg!(debug_assertions) {
        return;
    }
    for account in [
        device.legacy_clip_keychain_account(),
        device.keychain_account(),
    ] {
        if let Ok(old) = legacy_entry(&account) {
            let _ = old.delete_credential();
        }
    }
    if let Ok(legacy) = entry(&device.legacy_clip_keychain_account()) {
        let _ = legacy.delete_credential();
    }
}

/// Pairing secret of the clipboard agent on this device's target.
pub fn clip_secret(device: &SavedDevice) -> Option<cometdesk_clip::Secret> {
    cometdesk_clip::Secret::from_code(&load(device).ok()?.clip?)
}

pub fn set_clip_secret(
    device: &SavedDevice,
    secret: &cometdesk_clip::Secret,
) -> Result<(), String> {
    update(device, |c| c.clip = Some(secret.to_code()))
}

pub fn delete_clip_secret(device: &SavedDevice) {
    let _ = update(device, |c| c.clip = None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_round_trip_and_read_legacy_password() {
        let c = Credentials {
            password: Some("pw".into()),
            clip: Some("code".into()),
        };
        assert_eq!(Credentials::parse(&c.encode()), c);
        let old = Credentials::parse(&format!(
            "{LEGACY_BLOB_PREFIX}{{\"password\":\"pw\",\"clip\":\"code\"}}"
        ));
        assert_eq!(old, c);
        let legacy = Credentials::parse("{not: json}");
        assert_eq!(legacy.password.as_deref(), Some("{not: json}"));
        assert_eq!(legacy.clip, None);
    }

    fn device(host: &str) -> SavedDevice {
        SavedDevice {
            name: "Comet".into(),
            host: host.into(),
            user: "admin".into(),
            target: TargetOs::Linux,
            keymap: Some("de".into()),
            clip_direction: ClipDirection::ToTarget,
            insert_paste: cometdesk_clip::PastePref::CtrlShiftV,
            transport: String::new(),
            audio: false,
            audio_output: String::new(),
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("cometdesk-store-{}-{name}", std::process::id()))
            .join("devices.json")
    }

    #[test]
    fn missing_file_is_empty() {
        let s = Store::load(&temp_path("missing")).unwrap();
        assert!(s.devices.is_empty());
    }

    #[test]
    fn save_and_reload_without_secrets() {
        let path = temp_path("roundtrip");
        let mut s = Store::load(&path).unwrap();
        s.upsert(device("10.0.0.2"));
        s.save().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.to_lowercase().contains("password"), "{text}");
        let reloaded = Store::load(&path).unwrap();
        assert_eq!(reloaded.devices, vec![device("10.0.0.2")]);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn upsert_replaces_same_host_and_user() {
        let mut s = Store {
            path: None,
            devices: vec![],
        };
        s.upsert(device("a"));
        let mut renamed = device("a");
        renamed.name = "Office".into();
        s.upsert(renamed);
        s.upsert(device("b"));
        assert_eq!(s.devices.len(), 2);
        assert_eq!(s.devices[0].name, "Office");
        assert_eq!(s.remove(5), None);
        assert_eq!(s.remove(0).unwrap().host, "a");
    }

    #[test]
    fn unknown_fields_and_defaults() {
        let f: File =
            serde_json::from_str(r#"{"devices":[{"host":"h","extra":1}],"v":2}"#).unwrap();
        assert_eq!(f.devices[0].target, TargetOs::Windows);
        assert_eq!(f.devices[0].display_name(), "h");
    }

    #[test]
    fn target_os_key_options() {
        assert!(TargetOs::Windows.key_options(false).swap_cmd_ctrl);
        assert!(TargetOs::Linux.key_options(false).swap_cmd_ctrl);
        assert!(!TargetOs::MacOs.key_options(false).swap_cmd_ctrl);
        assert!(!TargetOs::Bios.key_options(true).swap_cmd_ctrl);
        assert!(TargetOs::Bios.key_options(true).iso_fix);
    }
}
