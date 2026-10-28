//! Start screen: saved devices, add/edit form, password and 2FA prompts.

use crate::store::{self, SavedDevice, Store, TargetOs};

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
    form: Option<Form>,
    prompt: Option<Prompt>,
    error: Option<String>,
    /// Password from `.env`/environment, used for the env device when the keychain has none.
    env_password: Option<(String, String, String)>,
}

impl DevicesUi {
    pub fn new(store: Store) -> Self {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let env_password = match (env("GLKVM_HOST"), env("GLKVM_PASSWORD")) {
            (Some(host), Some(pw)) => Some((
                host,
                env("GLKVM_USER").unwrap_or_else(|| "admin".into()),
                pw,
            )),
            _ => None,
        };
        let mut ui = Self {
            store,
            form: None,
            prompt: None,
            error: None,
            env_password,
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
    /// `GLKVM_AUTOCONNECT=1`: connect to the `.env` device at start (development aid; nothing
    /// is saved).
    pub fn autoconnect(&self) -> Option<ConnectRequest> {
        if std::env::var("GLKVM_AUTOCONNECT").ok().as_deref() != Some("1") {
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

    fn password_for(&self, device: &SavedDevice) -> Option<String> {
        store::password(device).or_else(|| {
            let (host, user, pw) = self.env_password.as_ref()?;
            (*host == device.host && *user == device.user).then(|| pw.clone())
        })
    }

    fn connect(&mut self, device: SavedDevice) -> Option<ConnectRequest> {
        self.error = None;
        match self.password_for(&device) {
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

    /// Draws the screen. Returns a connect request when the user starts one.
    pub fn ui(&mut self, ui: &mut egui::Ui, busy: Option<&str>) -> Option<ConnectRequest> {
        let mut request = None;
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("GLKVM devices");
            ui.add_space(8.0);
            if let Some(status) = busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(status);
                });
            }
            if let Some(e) = &self.error {
                ui.colored_label(egui::Color32::from_rgb(220, 80, 70), e);
            }
            ui.add_space(8.0);

            let mut connect = None;
            let mut edit = None;
            let mut remove = None;
            egui::Grid::new("devices")
                .num_columns(4)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    for (i, d) in self.store.devices.iter().enumerate() {
                        ui.strong(d.display_name());
                        ui.label(format!("{}@{}", d.user, d.host));
                        ui.label(d.target.label());
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(busy.is_none(), egui::Button::new("Connect"))
                                .clicked()
                            {
                                connect = Some(i);
                            }
                            if ui.button("Edit").clicked() {
                                edit = Some(i);
                            }
                            if ui.button("Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                        ui.end_row();
                    }
                });
            if let Some(i) = connect {
                request = self.connect(self.store.devices[i].clone());
            }
            if let Some(i) = edit {
                self.form = Some(Form {
                    editing: Some(i),
                    device: self.store.devices[i].clone(),
                    password: String::new(),
                    remember: true,
                });
            }
            if let Some(i) = remove
                && let Some(d) = self.store.remove(i)
            {
                store::delete_password(&d);
                self.save();
            }
            ui.add_space(12.0);
            if self.form.is_none() && ui.button("Add device").clicked() {
                let mut form = Form {
                    remember: true,
                    ..Default::default()
                };
                form.device.user = "admin".into();
                self.form = Some(form);
            }
            if let Some(r) = self.form_ui(ui) {
                request = Some(r);
            }
        });
        if let Some(r) = self.prompt_ui(ui.ctx()) {
            request = Some(r);
        }
        request
    }

    fn form_ui(&mut self, ui: &mut egui::Ui) -> Option<ConnectRequest> {
        let form = self.form.as_mut()?;
        let mut save = false;
        let mut cancel = false;
        ui.group(|ui| {
            ui.strong(if form.editing.is_some() {
                "Edit device"
            } else {
                "Add device"
            });
            egui::Grid::new("form")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut form.device.name);
                    ui.end_row();
                    ui.label("Address");
                    ui.add(
                        egui::TextEdit::singleline(&mut form.device.host)
                            .hint_text("192.168.8.1 or host:port"),
                    );
                    ui.end_row();
                    ui.label("User");
                    ui.text_edit_singleline(&mut form.device.user);
                    ui.end_row();
                    ui.label("Password");
                    ui.add(
                        egui::TextEdit::singleline(&mut form.password)
                            .password(true)
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
                    .add_enabled(valid, egui::Button::new("Save and connect"))
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
        form.device.host = glkvm::client::normalize_host(&form.device.host);
        form.device.user = form.device.user.trim().to_owned();
        if let Some(i) = form.editing {
            if let Some(old) = self.store.devices.get(i)
                && (old.host != form.device.host || old.user != form.device.user)
            {
                store::delete_password(old);
            }
            self.store.remove(i);
        }
        self.store.upsert(form.device.clone());
        if !form.password.is_empty()
            && form.remember
            && let Err(e) = store::set_password(&form.device, &form.password)
        {
            self.error = Some(format!("Could not save the password in the keychain: {e}"));
        }
        self.save();
        if form.password.is_empty() {
            self.connect(form.device)
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
        egui::Modal::new(egui::Id::unique("prompt")).show(ctx, |ui| match prompt {
            Prompt::Password {
                device,
                password,
                remember,
                wrong,
            } => {
                ui.heading(format!("Password for {}@{}", device.user, device.host));
                if *wrong {
                    ui.label("The saved password was rejected.");
                }
                let r = ui.add(egui::TextEdit::singleline(password).password(true));
                r.request_focus();
                submit = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.checkbox(remember, "Remember in the keychain");
                ui.horizontal(|ui| {
                    cancel = ui.button("Cancel").clicked();
                    submit |= ui.button("Connect").clicked();
                });
            }
            Prompt::Totp { device, code, .. } => {
                ui.heading(format!("2FA code for {}", device.display_name()));
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
                    submit |= ui.button("Connect").clicked();
                });
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
                if remember && let Err(e) = store::set_password(&device, &password) {
                    self.error = Some(format!("Could not save the password in the keychain: {e}"));
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
