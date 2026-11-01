//! Device screen state, connect requests/errors, and keychain credential lifecycle.
//!
//! Entry points: `DevicesUi::new`, `DevicesUi::connect_device`, `DevicesUi::on_password`.
//! Read `form_modal.rs` or `prompt.rs` for credential input dialogs.

use std::sync::Arc;

use tokio::runtime::Handle;
use winit::event_loop::EventLoopProxy;

use super::reachability::Probes;
use crate::app::UserEvent;
use crate::store::{self, SavedDevice, Store};

/// Why the last connect attempt didn't produce a session.
#[derive(Debug, Clone)]
pub enum ConnectError {
    WrongPassword,
    NeedTotp,
    Other(String),
}

#[derive(Debug, Clone)]
pub struct ConnectRequest {
    pub device: SavedDevice,
    pub password: String,
    pub totp: Option<String>,
}

#[derive(Default)]
pub(super) struct Form {
    /// Index being edited, or `None` for a new device.
    pub(super) editing: Option<usize>,
    pub(super) device: SavedDevice,
    pub(super) password: String,
    pub(super) remember: bool,
}

pub(super) enum Prompt {
    Password {
        device: SavedDevice,
        password: String,
        remember: bool,
        wrong: bool,
    },
    Totp {
        device: SavedDevice,
        password: String,
        code: String,
    },
}

pub struct DevicesUi {
    pub store: Store,
    pub(super) rt: Handle,
    pub(super) proxy: EventLoopProxy<UserEvent>,
    pub(super) form: Option<Form>,
    pub(super) prompt: Option<Prompt>,
    pub(super) error: Option<String>,
    /// Password from `.env`/environment, used for the env device when the keychain has none.
    pub(super) env_password: Option<(String, String, String)>,
    /// A keychain password read is in flight for this device (connect is waiting on it).
    pub(super) looking_up: Option<SavedDevice>,
    pub(super) logo: Option<egui::TextureHandle>,
    /// Reachability of each saved host: when it was probed, and the answer (`None` while
    /// the probe runs).
    pub(super) probes: Probes,
    /// Index of the device whose removal waits for confirmation.
    pub(super) confirm_remove: Option<usize>,
}

impl DevicesUi {
    pub(super) fn blank_form(&self) -> Form {
        let mut form = Form {
            remember: true,
            ..Default::default()
        };
        form.device.user = "admin".into();
        if self.store.devices.is_empty()
            && let Some((host, user, _)) = &self.env_password
        {
            form.device.host = host.clone();
            form.device.user = user.clone();
            form.device.name = "Comet".into();
        }
        form
    }

    pub fn new(store: Store, rt: Handle, proxy: EventLoopProxy<UserEvent>) -> Self {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let env_password = match (env("COMETDESK_HOST"), env("COMETDESK_PASSWORD")) {
            (Some(host), Some(pw)) => Some((
                host,
                env("COMETDESK_USER").unwrap_or_else(|| "admin".into()),
                pw,
            )),
            _ => None,
        };
        Self {
            store,
            rt,
            proxy,
            form: None,
            prompt: None,
            error: None,
            env_password,
            looking_up: None,
            logo: None,
            probes: Arc::default(),
            confirm_remove: None,
        }
    }

    /// Called when a connect attempt failed in a way the user can fix.
    /// `COMETDESK_AUTOCONNECT=1`: connect to the `.env` device at start (development aid; nothing
    /// is saved).
    pub fn autoconnect(&self) -> Option<ConnectRequest> {
        if std::env::var("COMETDESK_AUTOCONNECT").ok().as_deref() != Some("1") {
            return None;
        }
        let (host, user, password) = self.env_password.clone()?;
        let device = SavedDevice {
            name: "Comet (.env)".into(),
            host,
            user,
            ..Default::default()
        };
        Some(ConnectRequest {
            device,
            password,
            totp: None,
        })
    }

    pub fn connect_failed(&mut self, req: ConnectRequest, err: ConnectError) {
        match err {
            ConnectError::WrongPassword => {
                self.error = Some("Wrong user name or password.".into());
                self.prompt = Some(Prompt::Password {
                    device: req.device,
                    password: String::new(),
                    remember: true,
                    wrong: true,
                });
            }
            ConnectError::NeedTotp => {
                self.prompt = Some(Prompt::Totp {
                    device: req.device,
                    password: req.password,
                    code: String::new(),
                });
            }
            ConnectError::Other(e) => self.error = Some(e),
        }
    }

    /// The env password for this device, when it matches host and user.
    fn env_password_for(&self, device: &SavedDevice) -> Option<String> {
        let (host, user, pw) = self.env_password.as_ref()?;
        (*host == device.host && *user == device.user).then(|| pw.clone())
    }

    /// Starts connecting: reads the saved password off the UI thread (the macOS keychain can
    /// block on an ACL prompt) and continues in [`Self::on_password`].
    pub fn connect_device(&mut self, device: SavedDevice) {
        self.connect(device);
    }

    pub(super) fn connect(&mut self, device: SavedDevice) {
        self.error = None;
        self.looking_up = Some(device.clone());
        let proxy = self.proxy.clone();
        self.rt.spawn_blocking(move || {
            let password = store::password(&device);
            let _ = proxy.send_event(UserEvent::Password(device, password));
        });
    }

    /// The keychain read finished: connect with the password, or fall back to the env password,
    /// else ask for one.
    pub fn on_password(
        &mut self,
        device: SavedDevice,
        password: Option<String>,
    ) -> Option<ConnectRequest> {
        if self.looking_up.as_ref() != Some(&device) {
            return None; // superseded by another action
        }
        self.looking_up = None;
        match password.or_else(|| self.env_password_for(&device)) {
            Some(password) => Some(ConnectRequest {
                device,
                password,
                totp: None,
            }),
            None => {
                self.prompt = Some(Prompt::Password {
                    device,
                    password: String::new(),
                    remember: true,
                    wrong: false,
                });
                None
            }
        }
    }

    /// Saves a password in the keychain off the UI thread (fire and forget; errors are logged).
    pub(super) fn save_password(&self, device: SavedDevice, password: String) {
        self.rt.spawn_blocking(move || {
            if let Err(e) = store::set_password(&device, &password) {
                tracing::warn!(%e, "could not save the password in the keychain");
            }
        });
    }

    /// Removes a device's keychain credentials off the UI thread.
    pub(super) fn forget_password(&self, device: SavedDevice) {
        self.rt
            .spawn_blocking(move || store::delete_password(&device));
    }

    pub fn update_device(&mut self, device: &SavedDevice) {
        self.store.upsert(device.clone());
        self.save();
    }

    pub(super) fn save(&mut self) {
        if let Err(e) = self.store.save() {
            self.error = Some(format!("Could not save devices: {e}"));
        }
    }
}
