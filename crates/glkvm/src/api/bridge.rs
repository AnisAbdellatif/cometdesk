//! The clipboard-bridge relay (`/glkvm-bridge/*`). Not GL firmware: our kvmd extra from
//! `bridge/` in this repo (docs/clipboard-bridge.md). It sits outside `/api`, behind the same
//! nginx login check.
//!
//! [`setup`] installs, updates or repairs it through the device's web terminal
//! ([`crate::shell`]); the device-side files are embedded here at build time, so a client
//! always installs the relay version it was built against. [`health`] tells what is needed.

use std::path::Path;

use serde::Deserialize;

use crate::client::{Body, status_error};
use crate::{Error, Result, Session};

/// WebSocket path of the relay's client side, for [`Session::ws_connect`].
pub const BRIDGE_WS: &str = "glkvm-bridge/ws";

/// Relay version this build installs (`VERSION` in `bridge/glkvm_bridge.py`).
pub const BRIDGE_VERSION: &str = "0.3.1";

/// Device-side files, staged into one directory and installed by `setup.sh`.
const FILES: &[(&str, &str)] = &[
    ("setup.sh", include_str!("../../../../bridge/setup.sh")),
    ("remove.sh", include_str!("../../../../bridge/remove.sh")),
    (
        "glkvm_bridge.py",
        include_str!("../../../../bridge/glkvm_bridge.py"),
    ),
    (
        "S99glkvm-bridge",
        include_str!("../../../../bridge/S99glkvm-bridge"),
    ),
    (
        "udhcpd.conf",
        include_str!("../../../../bridge/udhcpd.conf"),
    ),
    (
        "90-glkvm-rs-rndis.yaml",
        include_str!("../../../../bridge/90-glkvm-rs-rndis.yaml"),
    ),
    (
        "manifest.yaml",
        include_str!("../../../../bridge/extra/manifest.yaml"),
    ),
    (
        "nginx.ctx-http.conf",
        include_str!("../../../../bridge/extra/nginx.ctx-http.conf"),
    ),
    (
        "nginx.ctx-server.conf",
        include_str!("../../../../bridge/extra/nginx.ctx-server.conf"),
    ),
    (
        "install.sh",
        include_str!("../../../../bridge/target/install.sh"),
    ),
    (
        "install.ps1",
        include_str!("../../../../bridge/target/install.ps1"),
    ),
];
const STAGE: &str = "/tmp/glkvm-setup";

/// Short hash of all device-side files this build installs. The device stores it, so any
/// change to them (not only a relay version bump) shows up as [`Health::Outdated`].
pub fn bundle_id() -> String {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    for (name, content) in FILES {
        h.update(name.as_bytes());
        h.update([0]);
        h.update(content.as_bytes());
        h.update([0]);
    }
    hex::encode(&h.finalize()[..6])
}

/// Agent binaries the target installers download (`bridge/target/install.*`).
pub const AGENT_NAMES: [&str; 3] = ["linux-x86_64", "linux-aarch64", "windows-x86_64.exe"];

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct BridgeSide {
    pub connected: bool,
    /// Paired with the other side in the current session.
    pub paired: bool,
    pub peer: Option<String>,
    pub sessions: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct UsbHealth {
    /// The RNDIS function is part of the USB gadget.
    pub gadget: bool,
    /// `usb0` exists on the device.
    pub iface: bool,
    /// A target is attached and has the link up.
    pub carrier: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct StoredAgent {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct BridgeStatus {
    pub version: String,
    /// How the agent reaches the relay (`rndis`).
    pub link: String,
    /// Device firmware (`RK_VERSION`); `None` from relays before 0.3.
    pub firmware: Option<String>,
    /// [`bundle_id`] of the build that installed the device files; `None` before 0.3.1.
    pub bundle: Option<String>,
    pub usb: Option<UsbHealth>,
    pub agents: Vec<StoredAgent>,
    pub agent: BridgeSide,
    pub client: BridgeSide,
}

/// What a device needs, from its status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    NotInstalled,
    /// Installed, but a different version than this build carries.
    Outdated {
        installed: String,
    },
    /// Installed, but something is missing (typically after a firmware upgrade).
    Broken(String),
    Ok,
}

impl Health {
    /// Whether [`setup`] would change something.
    pub fn needs_setup(&self) -> bool {
        *self != Health::Ok
    }
}

pub fn health(status: Option<&BridgeStatus>) -> Health {
    let Some(st) = status else {
        return Health::NotInstalled;
    };
    if st.version != BRIDGE_VERSION || st.bundle.as_deref().is_some_and(|b| b != bundle_id()) {
        return Health::Outdated {
            installed: st.version.clone(),
        };
    }
    match &st.usb {
        Some(u) if !u.gadget => Health::Broken("the USB network function is missing".into()),
        Some(u) if !u.iface => Health::Broken("the USB network interface is missing".into()),
        _ => Health::Ok,
    }
}

#[derive(Debug, Clone, Default)]
pub struct SetupReport {
    /// What setup changed, one line per change.
    pub changes: Vec<String>,
    pub version: String,
    /// The target's USB link was re-created (keyboard and mouse dropped for a moment).
    pub usb_restarted: bool,
}

impl Session {
    /// `GET /glkvm-bridge/status`. `None`: the bridge is not installed on this device.
    pub async fn bridge_status(&self) -> Result<Option<BridgeStatus>> {
        // The session's base URL is `https://<host>/api/`.
        let (status, body) = self
            .request(
                reqwest::Method::GET,
                "../glkvm-bridge/status",
                &[],
                Body::None,
            )
            .await?;
        match status {
            200 => Ok(Some(serde_json::from_slice(&body)?)),
            404 => Ok(None),
            s => Err(status_error(s, &body)),
        }
    }

    /// `PUT /glkvm-bridge/agents/<name>`: stores an agent binary for the target installers.
    pub async fn bridge_upload_agent(&self, name: &str, data: &[u8]) -> Result<StoredAgent> {
        if !AGENT_NAMES.contains(&name) {
            return Err(Error::Protocol(format!("unknown agent binary {name}")));
        }
        let (status, body) = self
            .request(
                reqwest::Method::PUT,
                &format!("../glkvm-bridge/agents/{name}"),
                &[],
                Body::Bytes(data),
            )
            .await?;
        if status != 200 {
            return Err(status_error(status, &body));
        }
        Ok(serde_json::from_slice(&body)?)
    }
}

/// Installs, updates or repairs the bridge on the device. Safe to run any time; changes only
/// what differs. `progress` receives short status lines.
pub async fn setup(session: &Session, progress: impl Fn(&str)) -> Result<SetupReport> {
    progress("Opening the device's terminal…");
    let mut sh = session.root_shell().await?;
    let result = async {
        sh.ok(&format!("rm -rf {STAGE} && mkdir -p {STAGE}"))
            .await?;
        for (name, content) in FILES {
            progress(&format!("Copying {name}…"));
            sh.upload(&format!("{STAGE}/{name}"), content.as_bytes(), "0644")
                .await?;
        }
        sh.upload(&format!("{STAGE}/bundle"), bundle_id().as_bytes(), "0644")
            .await?;
        progress("Installing…");
        let out = sh
            .run_timeout(
                // `(exit $s)` hands setup.sh's status to the shell's end marker.
                &format!("sh {STAGE}/setup.sh 2>&1; s=$?; rm -rf {STAGE}; (exit $s)"),
                std::time::Duration::from_secs(120),
            )
            .await?;
        parse_setup(&out.text, out.status)
    }
    .await;
    sh.close().await;
    result
}

fn parse_setup(text: &str, status: i32) -> Result<SetupReport> {
    let mut report = SetupReport::default();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(v) = line.strip_prefix("GLKVM_SETUP_OK ") {
            report.version = v.to_owned();
        } else {
            if line.starts_with("restarting kvmd-otg") {
                report.usb_restarted = true;
            }
            report.changes.push(line.to_owned());
        }
    }
    if status != 0 || report.version.is_empty() {
        return Err(Error::Protocol(format!(
            "bridge setup failed ({status}): {}",
            report.changes.join("; ")
        )));
    }
    Ok(report)
}

/// Removes the bridge and its USB network function from the device.
pub async fn remove(session: &Session) -> Result<()> {
    let mut sh = session.root_shell().await?;
    let result = async {
        let (_, script) = FILES.iter().find(|(n, _)| *n == "remove.sh").unwrap();
        sh.upload("/tmp/glkvm-remove.sh", script.as_bytes(), "0644")
            .await?;
        let out = sh
            .run_timeout(
                "sh /tmp/glkvm-remove.sh 2>&1; rm -f /tmp/glkvm-remove.sh",
                std::time::Duration::from_secs(60),
            )
            .await?;
        if out.text.contains("GLKVM_REMOVE_OK") {
            Ok(())
        } else {
            Err(Error::Protocol(format!(
                "bridge removal failed: {}",
                out.text.trim()
            )))
        }
    }
    .await;
    sh.close().await;
    result
}

/// Uploads the agent binaries found in `dir` (named as [`AGENT_NAMES`]) that the device
/// doesn't already have. Returns the names uploaded.
pub async fn sync_agents(
    session: &Session,
    dir: &Path,
    status: &BridgeStatus,
) -> Result<Vec<String>> {
    let mut uploaded = Vec::new();
    for name in AGENT_NAMES {
        let Ok(data) = std::fs::read(dir.join(name)) else {
            continue;
        };
        let sha = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(&data));
        if status
            .agents
            .iter()
            .any(|a| a.name == name && a.sha256 == sha)
        {
            continue;
        }
        let stored = session.bridge_upload_agent(name, &data).await?;
        if stored.sha256 != sha {
            return Err(Error::Protocol(format!("upload of {name} corrupted")));
        }
        uploaded.push(name.to_owned());
    }
    Ok(uploaded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_embedded_relay() {
        let (_, py) = FILES.iter().find(|(n, _)| *n == "glkvm_bridge.py").unwrap();
        assert!(
            py.contains(&format!("VERSION = \"{BRIDGE_VERSION}\"")),
            "update BRIDGE_VERSION to match bridge/glkvm_bridge.py"
        );
    }

    #[test]
    fn embedded_scripts_fit_the_installers() {
        for (name, _) in FILES {
            assert!(
                name.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            );
        }
        let (_, sh) = FILES.iter().find(|(n, _)| *n == "install.sh").unwrap();
        for agent in AGENT_NAMES.iter().filter(|a| a.starts_with("linux")) {
            assert!(sh.contains(agent), "install.sh must know {agent}");
        }
    }

    #[test]
    fn health_from_status() {
        assert_eq!(health(None), Health::NotInstalled);
        let mut st = BridgeStatus {
            version: "0.2.0".into(),
            ..Default::default()
        };
        assert!(matches!(health(Some(&st)), Health::Outdated { .. }));
        st.version = BRIDGE_VERSION.into();
        st.bundle = Some("000000000000".into());
        assert!(matches!(health(Some(&st)), Health::Outdated { .. }));
        st.bundle = Some(bundle_id());
        st.usb = Some(UsbHealth {
            gadget: false,
            ..Default::default()
        });
        assert!(matches!(health(Some(&st)), Health::Broken(_)));
        st.usb = Some(UsbHealth {
            gadget: true,
            iface: true,
            carrier: false,
        });
        assert_eq!(health(Some(&st)), Health::Ok);
    }

    #[test]
    fn setup_output_is_parsed() {
        let r = parse_setup(
            "updated /etc/x\nrestarting kvmd-otg (USB re-enumerates)\nGLKVM_SETUP_OK 0.3.0\n",
            0,
        )
        .unwrap();
        assert_eq!(r.version, "0.3.0");
        assert!(r.usb_restarted);
        assert_eq!(r.changes.len(), 2);
        assert!(parse_setup("nginx: [emerg] bad\n", 1).is_err());
    }
}
