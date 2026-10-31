//! Start screen: saved devices, add/edit form, password and 2FA prompts.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::runtime::Handle;
use winit::event_loop::EventLoopProxy;

use crate::app::UserEvent;
use crate::store::{self, SavedDevice, Store, TargetOs};
use crate::theme;

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
struct Form {
    /// Index being edited, or `None` for a new device.
    editing: Option<usize>,
    device: SavedDevice,
    password: String,
    remember: bool,
}

impl Form {
    fn blank() -> Self {
        let mut form = Form {
            remember: true,
            ..Default::default()
        };
        form.device.user = "admin".into();
        form
    }
}

enum Prompt {
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
    rt: Handle,
    proxy: EventLoopProxy<UserEvent>,
    form: Option<Form>,
    prompt: Option<Prompt>,
    error: Option<String>,
    /// Password from `.env`/environment, used for the env device when the keychain has none.
    env_password: Option<(String, String, String)>,
    /// A keychain password read is in flight for this device (connect is waiting on it).
    looking_up: Option<SavedDevice>,
    logo: Option<egui::TextureHandle>,
    /// Reachability of each saved host: when it was probed, and the answer (`None` while
    /// the probe runs).
    probes: Probes,
    /// Index of the device whose removal waits for confirmation.
    confirm_remove: Option<usize>,
}

type Probes = Arc<Mutex<HashMap<String, (Instant, Option<bool>)>>>;

const PROBE_TTL: Duration = Duration::from_secs(30);

/// Whether something answers on the device's HTTPS port (a quick, quiet TCP connect).
fn reachable(host: &str) -> bool {
    use std::net::ToSocketAddrs;
    let addr = if host.contains(':') {
        host.to_owned()
    } else {
        format!("{host}:443")
    };
    addr.to_socket_addrs().is_ok_and(|mut it| {
        it.any(|a| std::net::TcpStream::connect_timeout(&a, Duration::from_millis(1500)).is_ok())
    })
}

impl DevicesUi {
    /// Starts a probe for every saved host that has none or a stale one.
    fn probe_hosts(&self, ctx: &egui::Context) {
        let mut probes = self.probes.lock().unwrap();
        for d in &self.store.devices {
            let stale = probes
                .get(&d.host)
                .is_none_or(|(at, answer)| answer.is_some() && at.elapsed() > PROBE_TTL);
            if !stale {
                continue;
            }
            probes.insert(d.host.clone(), (Instant::now(), None));
            let (host, map, ctx) = (d.host.clone(), self.probes.clone(), ctx.clone());
            self.rt.spawn_blocking(move || {
                let up = reachable(&host);
                map.lock().unwrap().insert(host, (Instant::now(), Some(up)));
                ctx.request_repaint();
            });
        }
        // Come back to refresh stale answers while this screen is open.
        ctx.request_repaint_after(PROBE_TTL);
    }

    fn reachability(&self, host: &str) -> Option<bool> {
        self.probes.lock().unwrap().get(host).and_then(|(_, a)| *a)
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
        let mut ui = Self {
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
        };
        if ui.store.devices.is_empty() {
            // First run: prefill from the environment (developer convenience) or blank.
            let mut form = Form {
                remember: true,
                ..Default::default()
            };
            form.device.user = "admin".into();
            if let Some((host, user, _)) = &ui.env_password {
                form.device.host = host.clone();
                form.device.user = user.clone();
                form.device.name = "Comet".into();
            }
            ui.form = Some(form);
        }
        ui
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

    fn connect(&mut self, device: SavedDevice) {
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
    fn save_password(&self, device: SavedDevice, password: String) {
        self.rt.spawn_blocking(move || {
            if let Err(e) = store::set_password(&device, &password) {
                tracing::warn!(%e, "could not save the password in the keychain");
            }
        });
    }

    /// Removes a device's keychain credentials off the UI thread.
    fn forget_password(&self, device: SavedDevice) {
        self.rt
            .spawn_blocking(move || store::delete_password(&device));
    }

    /// Draws the screen. Returns a connect request when the user starts one.
    pub fn ui(&mut self, ui: &mut egui::Ui, busy: Option<&str>) -> Option<ConnectRequest> {
        let mut request = None;
        let logo = self
            .logo
            .get_or_insert_with(|| theme::logo_texture(ui.ctx()).expect("embedded logo decodes"))
            .clone();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::BG))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let width = ui.available_width().min(720.0);
                    let side = (ui.available_width() - width) / 2.0;
                    ui.horizontal_top(|ui| {
                        ui.add_space(side);
                        ui.vertical(|ui| {
                            ui.set_width(width);
                            request = self.content(ui, &logo, busy);
                        });
                    });
                });
            });
        if let Some(r) = self.prompt_ui(ui.ctx()) {
            request = Some(r);
        }
        request
    }

    fn content(
        &mut self,
        ui: &mut egui::Ui,
        logo: &egui::TextureHandle,
        busy: Option<&str>,
    ) -> Option<ConnectRequest> {
        let mut request = None;
        // While a keychain read is in flight, show it busy so Connect is disabled and a spinner
        // appears (the read runs off the UI thread).
        let busy = if self.looking_up.is_some() {
            Some("Checking the saved password…")
        } else {
            busy
        };
        ui.add_space(32.0);
        ui.horizontal(|ui| {
            ui.add(egui::Image::new(logo).fit_to_exact_size(egui::Vec2::splat(48.0)));
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(theme::sign("CometDesk", 32.0).strong().color(theme::TEXT));
                ui.label(egui::RichText::new("Remote console for GL.iNet KVM").color(theme::MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.form.is_none() && ui.add(egui::Button::new("Add device")).clicked() {
                    self.form = Some(Form::blank());
                }
            });
        });
        ui.add_space(24.0);
        if let Some(status) = busy {
            theme::card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(status);
                });
            });
            ui.add_space(8.0);
        }
        if let Some(e) = &self.error {
            theme::banner(ui, theme::ERR, e);
            ui.add_space(8.0);
        }

        self.probe_hosts(ui.ctx());
        let mut connect = None;
        let mut edit = None;
        let mut remove = None;
        // One cobalt action per screen: Connect on the first device that answers.
        let lead = self
            .store
            .devices
            .iter()
            .position(|d| self.reachability(&d.host) == Some(true))
            .unwrap_or(0);
        for (i, d) in self.store.devices.iter().enumerate() {
            let up = self.reachability(&d.host);
            theme::card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    theme::badge(ui, &d.host, d.display_name(), 52.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.horizontal(|ui| {
                            ui.label(theme::sign(d.display_name(), 20.0).strong());
                            match up {
                                Some(true) => theme::status_dot(ui, theme::OK, "Reachable"),
                                Some(false) => theme::status_dot(ui, theme::WARN, "Not reachable"),
                                None => {
                                    ui.label(
                                        egui::RichText::new("Checking…")
                                            .small()
                                            .color(theme::MUTED),
                                    );
                                }
                            }
                        });
                        ui.label(
                            egui::RichText::new(format!(
                                "{}@{}  ·  {}",
                                d.user,
                                d.host,
                                d.target.label()
                            ))
                            .small()
                            .color(theme::MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let button = if i == lead {
                            theme::primary("Connect")
                        } else {
                            egui::Button::new("Connect")
                        };
                        if ui.add_enabled(busy.is_none(), button).clicked() {
                            connect = Some(i);
                        }
                        ui.menu_button(egui::RichText::new("•••").color(theme::MUTED), |ui| {
                            if ui.button("Edit…").clicked() {
                                edit = Some(i);
                                ui.close();
                            }
                            if ui
                                .button(egui::RichText::new("Remove…").color(theme::ERR))
                                .clicked()
                            {
                                remove = Some(i);
                                ui.close();
                            }
                        });
                    });
                });
            });
        }
        if self.store.devices.is_empty() && self.form.is_none() {
            theme::card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(theme::sign("Add your Comet", 20.0).strong());
                ui.label(
                    egui::RichText::new(
                        "Enter its address and password. Over USB or its own Wi-Fi it is \
                         usually 192.168.200.1.",
                    )
                    .color(theme::MUTED),
                );
                ui.add_space(4.0);
                if ui.add(theme::primary("Add device")).clicked() {
                    self.form = Some(Form::blank());
                }
            });
        }
        if let Some(i) = connect {
            self.connect(self.store.devices[i].clone());
        }
        if let Some(i) = edit {
            self.form = Some(Form {
                editing: Some(i),
                device: self.store.devices[i].clone(),
                password: String::new(),
                remember: true,
            });
        }
        if remove.is_some() {
            self.confirm_remove = remove;
        }
        self.remove_dialog(ui.ctx());
        if let Some(r) = self.form_ui(ui) {
            request = Some(r);
        }
        ui.add_space(24.0);
        request
    }

    /// Asks before deleting a saved device: its keychain password goes with it.
    fn remove_dialog(&mut self, ctx: &egui::Context) {
        let Some(i) = self.confirm_remove else { return };
        let Some(device) = self.store.devices.get(i) else {
            self.confirm_remove = None;
            return;
        };
        let name = device.display_name();
        let (mut confirm, mut cancel) = (false, false);
        egui::Modal::new(egui::Id::unique("remove-device")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.label(
                egui::RichText::new(format!("Remove {name}?"))
                    .size(16.0)
                    .strong(),
            );
            ui.label("The device and its saved password are deleted from this Mac.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                cancel = ui.button("Keep").clicked();
                confirm = ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Remove")
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(theme::ERR.gamma_multiply(0.7)),
                    )
                    .clicked();
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if confirm && let Some(d) = self.store.remove(i) {
            self.forget_password(d);
            self.save();
        }
        if confirm || cancel {
            self.confirm_remove = None;
        }
    }

    fn form_ui(&mut self, ui: &mut egui::Ui) -> Option<ConnectRequest> {
        let form = self.form.as_mut()?;
        let mut save = false;
        let mut cancel = false;
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new(if form.editing.is_some() {
                    "Edit device"
                } else {
                    "Add device"
                })
                .size(16.0)
                .strong(),
            );
            egui::Grid::new("form")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.add(
                        egui::TextEdit::singleline(&mut form.device.name)
                            .desired_width(f32::INFINITY)
                            .hint_text("Office Comet"),
                    );
                    ui.end_row();
                    ui.label("Address");
                    ui.add(
                        egui::TextEdit::singleline(&mut form.device.host)
                            .desired_width(f32::INFINITY)
                            .hint_text("192.168.200.1 or host:port"),
                    );
                    ui.end_row();
                    ui.label("User");
                    ui.add(
                        egui::TextEdit::singleline(&mut form.device.user)
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();
                    ui.label("Password");
                    ui.add(
                        egui::TextEdit::singleline(&mut form.password)
                            .password(true)
                            .desired_width(f32::INFINITY)
                            .hint_text(if form.editing.is_some() {
                                "unchanged"
                            } else {
                                ""
                            }),
                    );
                    ui.end_row();
                    ui.label("Target");
                    egui::ComboBox::from_id_salt("form-target")
                        .selected_text(form.device.target.label())
                        .show_ui(ui, |ui| {
                            for os in TargetOs::ALL {
                                ui.selectable_value(&mut form.device.target, os, os.label());
                            }
                        });
                    ui.end_row();
                });
            ui.checkbox(&mut form.remember, "Remember password in the keychain");
            ui.horizontal(|ui| {
                let valid =
                    !form.device.host.trim().is_empty() && !form.device.user.trim().is_empty();
                save = ui
                    .add_enabled(valid, theme::primary("Save and connect"))
                    .clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if cancel {
            self.form = None;
            return None;
        }
        if !save {
            return None;
        }
        let mut form = self.form.take()?;
        form.device.host = cometdesk::client::normalize_host(&form.device.host);
        form.device.user = form.device.user.trim().to_owned();
        if let Some(i) = form.editing {
            if let Some(old) = self.store.devices.get(i)
                && (old.host != form.device.host || old.user != form.device.user)
            {
                self.forget_password(old.clone());
            }
            self.store.remove(i);
        }
        self.store.upsert(form.device.clone());
        if !form.password.is_empty() && form.remember {
            self.save_password(form.device.clone(), form.password.clone());
        }
        self.save();
        if form.password.is_empty() {
            self.connect(form.device);
            None
        } else {
            Some(ConnectRequest {
                device: form.device,
                password: form.password,
                totp: None,
            })
        }
    }

    fn prompt_ui(&mut self, ctx: &egui::Context) -> Option<ConnectRequest> {
        let prompt = self.prompt.as_mut()?;
        let mut submit = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("prompt")).show(ctx, |ui| {
            ui.set_width(340.0);
            match prompt {
                Prompt::Password {
                    device,
                    password,
                    remember,
                    wrong,
                } => {
                    ui.label(
                        egui::RichText::new(format!(
                            "Password for {}@{}",
                            device.user, device.host
                        ))
                        .size(17.0)
                        .strong(),
                    );
                    if *wrong {
                        theme::banner(ui, theme::ERR, "The saved password was rejected.");
                    }
                    let r = ui.add(egui::TextEdit::singleline(password).password(true));
                    r.request_focus();
                    submit = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.checkbox(remember, "Remember in the keychain");
                    ui.horizontal(|ui| {
                        cancel = ui.button("Cancel").clicked();
                        submit |= ui.add(theme::primary("Connect")).clicked();
                    });
                }
                Prompt::Totp { device, code, .. } => {
                    ui.label(
                        egui::RichText::new(format!("2FA code for {}", device.display_name()))
                            .size(17.0)
                            .strong(),
                    );
                    ui.label("The device has two-factor authentication enabled.");
                    let r = ui.add(
                        egui::TextEdit::singleline(code)
                            .char_limit(6)
                            .hint_text("123456"),
                    );
                    r.request_focus();
                    submit = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.horizontal(|ui| {
                        cancel = ui.button("Cancel").clicked();
                        submit |= ui.add(theme::primary("Connect")).clicked();
                    });
                }
            }
        });
        if cancel {
            self.prompt = None;
            return None;
        }
        if !submit {
            return None;
        }
        match self.prompt.take()? {
            Prompt::Password {
                device,
                password,
                remember,
                ..
            } => {
                if remember {
                    self.save_password(device.clone(), password.clone());
                }
                Some(ConnectRequest {
                    device,
                    password,
                    totp: None,
                })
            }
            Prompt::Totp {
                device,
                password,
                code,
            } => Some(ConnectRequest {
                device,
                password,
                totp: Some(code.trim().to_owned()),
            }),
        }
    }

    pub fn update_device(&mut self, device: &SavedDevice) {
        self.store.upsert(device.clone());
        self.save();
    }

    fn save(&mut self) {
        if let Err(e) = self.store.save() {
            self.error = Some(format!("Could not save devices: {e}"));
        }
    }
}
