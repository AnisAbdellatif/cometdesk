//! Saved devices (`<config dir>/glkvm/devices.json`) and their passwords (OS keychain).
//!
//! Passwords never touch the JSON file. Auth tokens are kept in memory only.

use std::io;
use std::path::{Path, PathBuf};

use glkvm_view::KeyOptions;
use serde::{Deserialize, Serialize};

const KEYCHAIN_SERVICE: &str = "glkvm";

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
}

impl SavedDevice {
    fn keychain_account(&self) -> String {
        format!("{}@{}", self.user, self.host)
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
        let path = glkvm::tls::config_dir().map(|d| d.join("glkvm").join("devices.json"));
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

pub fn password(device: &SavedDevice) -> Option<String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &device.keychain_account()).ok()?;
    entry.get_password().ok()
}

pub fn set_password(device: &SavedDevice, password: &str) -> Result<(), String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &device.keychain_account())
        .and_then(|e| e.set_password(password))
        .map_err(|e| e.to_string())
}

pub fn delete_password(device: &SavedDevice) {
    if let Ok(entry) = keyring::Entry::new(KEYCHAIN_SERVICE, &device.keychain_account()) {
        let _ = entry.delete_credential();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(host: &str) -> SavedDevice {
        SavedDevice {
            name: "Comet".into(),
            host: host.into(),
            user: "admin".into(),
            target: TargetOs::Linux,
            keymap: Some("de".into()),
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("glkvm-store-{}-{name}", std::process::id()))
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
