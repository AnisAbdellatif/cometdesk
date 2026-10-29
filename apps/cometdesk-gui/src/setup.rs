//! Device-side setup of the clipboard bridge: health check on connect, and one-click
//! install / update / repair through the device's web terminal (`cometdesk::api::bridge::setup`).
//! Also uploads the agent binaries the target installers download.
//!
//! Firmware upgrades may wipe what we installed; the check on every connect notices, and
//! the same idempotent setup repairs it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cometdesk::Session;
use cometdesk::api::bridge::{self, BridgeStatus, Health};
use tokio::runtime::Handle;

#[derive(Debug, Clone, Default)]
struct Inner {
    checked: bool,
    status: Option<BridgeStatus>,
    /// Progress line while setup runs.
    running: Option<String>,
    /// Outcome of the last setup run, for the UI to show once.
    outcome: Option<Result<String, String>>,
}

#[derive(Clone)]
pub struct BridgeSetup {
    inner: Arc<Mutex<Inner>>,
    session: Session,
    rt: Handle,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl BridgeSetup {
    pub fn new(rt: Handle, session: Session, wake: impl Fn() + Send + Sync + 'static) -> Self {
        let s = BridgeSetup {
            inner: Arc::default(),
            session,
            rt,
            wake: Arc::new(wake),
        };
        s.check();
        s
    }

    /// Refreshes the status in the background.
    /// Refreshes the status in the background. When the bridge is healthy but holds older
    /// agent programs than this app bundles, uploads them quietly (no USB restart, so no
    /// need to ask): targets then get the current agent when they (re)install it.
    pub fn check(&self) {
        let this = self.clone();
        self.rt.spawn(async move {
            let mut status = this.session.bridge_status().await;
            if let Ok(Some(st)) = &status
                && bridge::health(Some(st)) == Health::Ok
                && let Some(dir) = agents_dir()
            {
                match bridge::sync_agents(&this.session, &dir, st).await {
                    Ok(up) if !up.is_empty() => {
                        tracing::info!(agents = ?up, "updated the agent programs on the Comet");
                        status = this.session.bridge_status().await;
                    }
                    Ok(_) => {}
                    Err(e) => tracing::warn!(%e, "cannot update the agent programs on the Comet"),
                }
            }
            {
                let mut i = this.inner.lock().unwrap();
                i.checked = true;
                match status {
                    Ok(s) => i.status = s,
                    Err(e) => tracing::warn!(%e, "cannot read the bridge status"),
                }
            }
            (this.wake)();
        });
    }

    /// `None` until the first check finished.
    pub fn health(&self) -> Option<Health> {
        let i = self.inner.lock().unwrap();
        i.checked.then(|| bridge::health(i.status.as_ref()))
    }

    pub fn status(&self) -> Option<BridgeStatus> {
        self.inner.lock().unwrap().status.clone()
    }

    pub fn progress(&self) -> Option<String> {
        self.inner.lock().unwrap().running.clone()
    }

    pub fn take_outcome(&self) -> Option<Result<String, String>> {
        self.inner.lock().unwrap().outcome.take()
    }

    /// Whether the device holds the agent binary the target installer will download.
    pub fn has_agent(&self, name: &str) -> bool {
        self.inner
            .lock()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.agents.iter().any(|a| a.name == name))
    }

    /// Installs, updates or repairs the bridge, then uploads the agent binaries.
    pub fn run(&self) {
        {
            let mut i = self.inner.lock().unwrap();
            if i.running.is_some() {
                return;
            }
            i.running = Some("Starting…".into());
        }
        (self.wake)();
        let this = self.clone();
        self.rt.spawn(async move {
            let progress = |msg: &str| {
                this.inner.lock().unwrap().running = Some(msg.to_owned());
                (this.wake)();
            };
            let outcome = async {
                let report = bridge::setup(&this.session, progress)
                    .await
                    .map_err(|e| e.to_string())?;
                let status = this
                    .session
                    .bridge_status()
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or("the bridge does not answer after setup")?;
                let mut msg = format!("Clipboard bridge {} ready", report.version);
                match agents_dir() {
                    Some(dir) => {
                        progress("Uploading the agent programs…");
                        let up = bridge::sync_agents(&this.session, &dir, &status)
                            .await
                            .map_err(|e| e.to_string())?;
                        if !up.is_empty() {
                            msg += &format!("; uploaded {}", up.join(", "));
                        }
                    }
                    None => msg += "; no agent programs bundled with this app",
                }
                if report.usb_restarted {
                    msg += " (the target's USB devices were re-created)";
                }
                Ok::<_, String>(msg)
            }
            .await;
            match &outcome {
                Ok(m) => tracing::info!("{m}"),
                Err(e) => tracing::warn!(%e, "bridge setup failed"),
            }
            {
                let mut i = this.inner.lock().unwrap();
                i.running = None;
                i.outcome = Some(outcome);
            }
            this.check();
        });
    }
}

/// The agent program for a target running `os` on `arch` (as in its hello), if this app ships
/// one.
pub fn agent_binary(os: &str, arch: &str) -> Option<PathBuf> {
    let name = match (os, arch) {
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        ("windows", "x86_64") => "windows-x86_64.exe",
        _ => return None,
    };
    Some(agents_dir()?.join(name)).filter(|p| p.is_file())
}

/// Where the agent binaries (`scripts/build-agents.sh`) are: `$COMETDESK_AGENTS_DIR`, next to
/// the executable, in the app bundle's Resources, or the repo's `dist/agents` in development.
fn agents_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(d) = std::env::var_os("COMETDESK_AGENTS_DIR") {
        candidates.push(PathBuf::from(d));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("agents"));
        candidates.push(dir.join("../Resources/agents"));
    }
    candidates.push(PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../dist/agents"
    )));
    candidates
        .into_iter()
        .find(|d| bridge::AGENT_NAMES.iter().any(|n| d.join(n).is_file()))
}
