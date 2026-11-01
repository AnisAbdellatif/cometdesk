//! Start screen: saved devices, add/edit form, password and 2FA prompts.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{
    Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2,
};
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

/// Helper for menu rows with an icon and label.
fn menu_item(
    ui: &mut egui::Ui,
    icon: theme::Icon,
    text: &str,
    text_color: Color32,
) -> egui::Response {
    let desired_size = Vec2::new(ui.available_width().max(140.0), 34.0);
    let (rect, resp) = ui.allocate_exact_size(desired_size, Sense::click());
    if ui.is_rect_visible(rect) {
        if resp.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), theme::BORDER);
        }
        let icon_rect = Rect::from_center_size(
            Pos2::new(rect.min.x + 18.0, rect.center().y),
            Vec2::splat(14.0),
        );
        theme::draw_icon(ui.painter(), icon_rect, icon, text_color);
        ui.painter().text(
            Pos2::new(rect.min.x + 36.0, rect.center().y),
            Align2::LEFT_CENTER,
            text,
            FontId::new(13.5, FontFamily::Proportional),
            text_color,
        );
    }
    resp
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

    fn blank_form(&self) -> Form {
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

        let avail_w = ui.available_width();
        let max_w = 1168.0_f32;
        let side = ((avail_w - max_w) / 2.0).max(56.0);
        let body_w = (avail_w - 2.0 * side).min(max_w);

        // 1. Top header panel
        let header_frame = egui::Frame::NONE
            .fill(theme::PANEL)
            .stroke(Stroke::new(1.0, theme::BORDER))
            .inner_margin(egui::Margin::symmetric(40, 16));
        egui::Panel::top("connect_header")
            .frame(header_frame)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(side);
                    ui.set_width(body_w);
                    ui.add(egui::Image::new(&logo).fit_to_exact_size(Vec2::splat(40.0)));
                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(theme::bold("CometDesk", 20.0).color(theme::TEXT));
                        ui.label(
                            egui::RichText::new("Remote console for GL.iNet KVM")
                                .size(13.0)
                                .color(theme::MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let add_btn_size = Vec2::new(124.0, 38.0);
                        let (rect, resp) = ui.allocate_exact_size(add_btn_size, Sense::click());
                        if ui.is_rect_visible(rect) {
                            let hovered = resp.hovered();
                            let bg = if hovered {
                                theme::CARD_HOVER
                            } else {
                                theme::CARD
                            };
                            let border = if hovered {
                                theme::ACCENT
                            } else {
                                theme::BORDER
                            };
                            ui.painter().rect(
                                rect,
                                CornerRadius::same(8),
                                bg,
                                Stroke::new(1.0, border),
                                StrokeKind::Inside,
                            );
                            let icon_rect = Rect::from_center_size(
                                Pos2::new(rect.min.x + 22.0, rect.center().y),
                                Vec2::splat(14.0),
                            );
                            theme::draw_icon(
                                ui.painter(),
                                icon_rect,
                                theme::Icon::Plus,
                                theme::ACCENT,
                            );
                            ui.painter().text(
                                Pos2::new(rect.min.x + 38.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                "Add device",
                                FontId::new(14.0, theme::bold_family()),
                                theme::TEXT,
                            );
                        }
                        if resp.clicked() {
                            self.form = Some(self.blank_form());
                        }
                    });
                    ui.add_space(side);
                });
            });

        // 2. Bottom footer panel
        let footer_frame = egui::Frame::NONE
            .fill(theme::PANEL)
            .stroke(Stroke::new(1.0, theme::BORDER))
            .inner_margin(egui::Margin::symmetric(40, 16));
        egui::Panel::bottom("connect_footer")
            .frame(footer_frame)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(side);
                    ui.set_width(body_w);
                    ui.label(
                        egui::RichText::new("First time? A new Comet answers at ")
                            .size(13.0)
                            .color(theme::MUTED),
                    );
                    ui.label(theme::mono("192.168.200.1", 13.0).color(theme::TEXT));
                    ui.label(
                        egui::RichText::new(" over USB or its own Wi-Fi.")
                            .size(13.0)
                            .color(theme::MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let version = format!("v{}", env!("CARGO_PKG_VERSION"));
                        ui.label(egui::RichText::new(version).size(13.0).color(theme::MUTED));
                        ui.add_space(20.0);
                        ui.label(
                            egui::RichText::new("Passwords stay in the macOS keychain")
                                .size(13.0)
                                .color(theme::MUTED),
                        );
                    });
                    ui.add_space(side);
                });
            });

        // 3. Central body content
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::BG))
            .show(ui, |ui| {
                self.content(ui, busy, side, body_w);
            });

        // 4. Modals on top
        if let Some(r) = self.form_modal(ui.ctx()) {
            request = Some(r);
        }
        if let Some(r) = self.prompt_ui(ui.ctx()) {
            request = Some(r);
        }
        self.remove_dialog(ui.ctx());

        request
    }

    fn content(&mut self, ui: &mut egui::Ui, busy: Option<&str>, side: f32, body_w: f32) {
        let busy = if self.looking_up.is_some() {
            Some("Checking the saved password…")
        } else {
            busy
        };

        self.probe_hosts(ui.ctx());

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(side);
                ui.vertical(|ui| {
                    ui.set_width(body_w);
                    ui.add_space(32.0);

                    if let Some(status) = busy {
                        theme::panel_card().show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(egui::RichText::new(status).color(theme::TEXT));
                            });
                        });
                        ui.add_space(16.0);
                    }

                    if let Some(e) = &self.error {
                        theme::banner(ui, theme::ERR, e);
                        ui.add_space(16.0);
                    }

                    if self.store.devices.is_empty() {
                        // Empty state: centered card
                        ui.add_space(20.0);
                        ui.vertical_centered(|ui| {
                            let card_w = body_w.min(460.0);
                            ui.set_width(card_w);
                            theme::panel_card()
                                .inner_margin(egui::Margin::same(24))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.label(
                                        theme::bold("Add your Comet", 20.0).color(theme::TEXT),
                                    );
                                    ui.add_space(8.0);
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            egui::RichText::new(
                                                "Enter its address and password. Over USB or \
                                                 its own Wi-Fi it is usually ",
                                            )
                                            .size(13.5)
                                            .color(theme::MUTED),
                                        );
                                        ui.label(
                                            theme::mono("192.168.200.1", 13.5).color(theme::TEXT),
                                        );
                                        ui.label(
                                            egui::RichText::new(".").size(13.5).color(theme::MUTED),
                                        );
                                    });
                                    ui.add_space(16.0);
                                    if ui
                                        .add(
                                            theme::primary("Add device")
                                                .min_size(Vec2::new(120.0, 36.0)),
                                        )
                                        .clicked()
                                    {
                                        self.form = Some(self.blank_form());
                                    }
                                });
                        });
                    } else {
                        // Caption row
                        ui.horizontal(|ui| {
                            ui.label(
                                theme::bold(
                                    format!("YOUR DEVICES · {}", self.store.devices.len()),
                                    12.5,
                                )
                                .color(theme::MUTED),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new("Reachability is probed every 30 s")
                                            .size(12.5)
                                            .color(theme::MUTED),
                                    );
                                },
                            );
                        });
                        ui.add_space(20.0);

                        let mut connect = None;
                        let mut edit = None;
                        let mut remove = None;

                        // One yellow action per screen: Connect on the first reachable device
                        let lead = self
                            .store
                            .devices
                            .iter()
                            .position(|d| self.reachability(&d.host) == Some(true))
                            .unwrap_or(0);

                        let columns = if body_w < 700.0 { 1 } else { 2 };
                        let total_items = self.store.devices.len() + 1; // devices + Add tile
                        let gap = 20.0;
                        let col_w = if columns == 1 {
                            body_w
                        } else {
                            (body_w - gap) / 2.0
                        };
                        let col_h = 176.0;

                        let n_rows = total_items.div_ceil(columns);
                        for r in 0..n_rows {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = gap;
                                for c in 0..columns {
                                    let idx = r * columns + c;
                                    if idx >= total_items {
                                        break;
                                    }
                                    if idx < self.store.devices.len() {
                                        let i = idx;
                                        let d = &self.store.devices[i];
                                        let up = self.reachability(&d.host);
                                        let card_frame = theme::panel_card()
                                            .inner_margin(egui::Margin::symmetric(24, 20));
                                        card_frame.show(ui, |ui| {
                                            ui.vertical(|ui| {
                                                ui.set_width(col_w - 48.0);
                                                ui.set_min_height(col_h - 40.0);
                                                // Top row: badge, name + mono address, reachability
                                                ui.horizontal(|ui| {
                                                    theme::badge(
                                                        ui,
                                                        &d.host,
                                                        d.display_name(),
                                                        48.0,
                                                    );
                                                    ui.add_space(14.0);
                                                    ui.vertical(|ui| {
                                                        ui.spacing_mut().item_spacing.y = 3.0;
                                                        ui.label(
                                                            theme::bold(d.display_name(), 18.0)
                                                                .color(theme::TEXT),
                                                        );
                                                        ui.label(
                                                            theme::mono(
                                                                format!(
                                                                    "{}@{} · {}",
                                                                    d.user,
                                                                    d.host,
                                                                    d.target.label()
                                                                ),
                                                                12.5,
                                                            )
                                                            .color(theme::MUTED),
                                                        );
                                                    });
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            ui.with_layout(
                                                                egui::Layout::left_to_right(
                                                                    egui::Align::Center,
                                                                ),
                                                                |ui| match up {
                                                                    Some(true) => {
                                                                        theme::status_dot(
                                                                            ui,
                                                                            theme::OK,
                                                                            "Reachable",
                                                                        )
                                                                    }
                                                                    Some(false) => {
                                                                        theme::status_dot(
                                                                            ui,
                                                                            theme::WARN,
                                                                            "Not reachable",
                                                                        )
                                                                    }
                                                                    None => theme::status_dot(
                                                                        ui,
                                                                        theme::MUTED,
                                                                        "Checking…",
                                                                    ),
                                                                },
                                                            );
                                                        },
                                                    );
                                                });

                                                ui.add_space(
                                                    (ui.available_height() - 36.0).max(20.0),
                                                );

                                                // Second row: Connect + More button
                                                ui.horizontal(|ui| {
                                                    let spacing = 10.0;
                                                    let more_size = 36.0;
                                                    let connect_width = (ui.available_width()
                                                        - more_size
                                                        - spacing)
                                                        .max(60.0);

                                                    let button = if i == lead {
                                                        theme::primary("Connect").min_size(
                                                            Vec2::new(connect_width, 36.0),
                                                        )
                                                    } else {
                                                        egui::Button::new(
                                                            egui::RichText::new("Connect")
                                                                .font(FontId::new(
                                                                    14.0,
                                                                    theme::bold_family(),
                                                                ))
                                                                .color(theme::TEXT),
                                                        )
                                                        .fill(theme::CARD_HOVER)
                                                        .stroke(Stroke::new(1.0, theme::BORDER))
                                                        .corner_radius(CornerRadius::same(6))
                                                        .min_size(Vec2::new(connect_width, 36.0))
                                                    };

                                                    if ui
                                                        .add_enabled(busy.is_none(), button)
                                                        .clicked()
                                                    {
                                                        connect = Some(i);
                                                    }

                                                    let (more_rect, more_resp) = ui
                                                        .allocate_exact_size(
                                                            Vec2::splat(more_size),
                                                            Sense::click(),
                                                        );
                                                    if ui.is_rect_visible(more_rect) {
                                                        let is_open = egui::Popup::is_id_open(
                                                            ui.ctx(),
                                                            egui::Popup::default_response_id(
                                                                &more_resp,
                                                            ),
                                                        );
                                                        let highlighted =
                                                            more_resp.hovered() || is_open;
                                                        let bg = theme::CARD_HOVER;
                                                        let border = if highlighted {
                                                            theme::TEXT
                                                        } else {
                                                            theme::BORDER
                                                        };
                                                        ui.painter().rect(
                                                            more_rect,
                                                            CornerRadius::same(6),
                                                            bg,
                                                            Stroke::new(1.0, border),
                                                            StrokeKind::Inside,
                                                        );
                                                        theme::draw_icon(
                                                            ui.painter(),
                                                            more_rect,
                                                            theme::Icon::More,
                                                            if highlighted {
                                                                theme::TEXT
                                                            } else {
                                                                theme::MUTED
                                                            },
                                                        );
                                                    }

                                                    let popup_frame = egui::Frame::NONE
                                                        .fill(theme::CARD_HOVER)
                                                        .stroke(Stroke::new(1.0, theme::BORDER))
                                                        .corner_radius(CornerRadius::same(10))
                                                        .inner_margin(egui::Margin::same(6));
                                                    egui::Popup::menu(&more_resp)
                                                        .frame(popup_frame)
                                                        .show(|ui| {
                                                            ui.set_min_width(140.0);
                                                            if menu_item(
                                                                ui,
                                                                theme::Icon::Pencil,
                                                                "Edit…",
                                                                theme::TEXT,
                                                            )
                                                            .clicked()
                                                            {
                                                                edit = Some(i);
                                                                ui.close();
                                                            }
                                                            if menu_item(
                                                                ui,
                                                                theme::Icon::Trash,
                                                                "Remove…",
                                                                theme::ERR,
                                                            )
                                                            .clicked()
                                                            {
                                                                remove = Some(i);
                                                                ui.close();
                                                            }
                                                        });
                                                });
                                            });
                                        });
                                    } else {
                                        // "Add another Comet" dashed tile
                                        let (add_tile_rect, add_tile_resp) = ui
                                            .allocate_exact_size(
                                                Vec2::new(col_w, col_h),
                                                Sense::click(),
                                            );
                                        if ui.is_rect_visible(add_tile_rect) {
                                            let hovered = add_tile_resp.hovered();
                                            let bg = if hovered {
                                                theme::CARD_HOVER
                                            } else {
                                                Color32::TRANSPARENT
                                            };
                                            let border_color = if hovered {
                                                theme::ACCENT
                                            } else {
                                                theme::BORDER
                                            };
                                            if hovered {
                                                ui.painter().rect_filled(
                                                    add_tile_rect,
                                                    CornerRadius::same(10),
                                                    bg,
                                                );
                                            }
                                            theme::draw_dashed_rect(
                                                ui.painter(),
                                                add_tile_rect,
                                                10.0,
                                                Stroke::new(1.5, border_color),
                                                7.0,
                                                6.0,
                                            );
                                            let center = add_tile_rect.center();
                                            let plus_center = Pos2::new(center.x, center.y - 14.0);
                                            let plus_rect = Rect::from_center_size(
                                                plus_center,
                                                Vec2::splat(20.0),
                                            );
                                            theme::draw_icon(
                                                ui.painter(),
                                                plus_rect,
                                                theme::Icon::Plus,
                                                theme::ACCENT,
                                            );
                                            ui.painter().text(
                                                Pos2::new(center.x, center.y + 16.0),
                                                Align2::CENTER_CENTER,
                                                "Add another Comet",
                                                FontId::new(14.5, theme::bold_family()),
                                                if hovered { theme::TEXT } else { theme::MUTED },
                                            );
                                        }
                                        if add_tile_resp.clicked() {
                                            self.form = Some(self.blank_form());
                                        }
                                    }
                                }
                            });
                            ui.add_space(gap);
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
                    }

                    ui.add_space(32.0);
                });
                ui.add_space(side);
            });
        });
    }

    /// Add/Edit device form shown as a centered modal dialog.
    fn form_modal(&mut self, ctx: &egui::Context) -> Option<ConnectRequest> {
        let form = self.form.as_mut()?;
        let mut save = false;
        let mut cancel = false;
        let is_editing = form.editing.is_some();

        egui::Modal::new(egui::Id::unique("device-form"))
            .frame(theme::modal_frame())
            .backdrop_color(Color32::from_black_alpha(180))
            .show(ctx, |ui| {
                ui.set_width(440.0);
                ui.label(
                    theme::bold(
                        if is_editing {
                            "Edit device"
                        } else {
                            "Add device"
                        },
                        18.0,
                    )
                    .color(theme::TEXT),
                );

                if !is_editing {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "A new Comet answers at 192.168.200.1 over USB or its own Wi-Fi.",
                        )
                        .size(13.0)
                        .color(theme::MUTED),
                    );
                }

                ui.add_space(16.0);

                // Name
                theme::field_label(ui, "Name");
                ui.add(
                    egui::TextEdit::singleline(&mut form.device.name)
                        .hint_text("Office Comet")
                        .desired_width(f32::INFINITY)
                        .min_size(Vec2::new(0.0, 36.0)),
                );

                ui.add_space(12.0);

                // Address & User side-by-side (Address wider)
                ui.horizontal(|ui| {
                    let gap = 12.0;
                    let user_w = 130.0;
                    let addr_w = (ui.available_width() - user_w - gap).max(100.0);

                    ui.vertical(|ui| {
                        ui.set_width(addr_w);
                        theme::field_label(ui, "Address");
                        ui.add(
                            egui::TextEdit::singleline(&mut form.device.host)
                                .hint_text("192.168.200.1 or host:port")
                                .font(FontId::new(13.0, FontFamily::Monospace))
                                .desired_width(f32::INFINITY)
                                .min_size(Vec2::new(0.0, 36.0)),
                        );
                    });
                    ui.add_space(gap);
                    ui.vertical(|ui| {
                        ui.set_width(user_w);
                        theme::field_label(ui, "User");
                        ui.add(
                            egui::TextEdit::singleline(&mut form.device.user)
                                .hint_text("admin")
                                .desired_width(f32::INFINITY)
                                .min_size(Vec2::new(0.0, 36.0)),
                        );
                    });
                });

                ui.add_space(12.0);

                // Password
                theme::field_label(ui, "Password");
                ui.add(
                    egui::TextEdit::singleline(&mut form.password)
                        .password(true)
                        .hint_text(if is_editing { "unchanged" } else { "" })
                        .desired_width(f32::INFINITY)
                        .min_size(Vec2::new(0.0, 36.0)),
                );

                ui.add_space(12.0);

                // Target runs
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label(
                        egui::RichText::new("Target runs")
                            .font(FontId::new(12.0, theme::bold_family()))
                            .color(theme::MUTED),
                    );
                    ui.label(
                        egui::RichText::new(" · sets shortcuts and paste behaviour")
                            .size(12.0)
                            .color(theme::MUTED),
                    );
                });
                ui.add_space(6.0);
                let mut seg_idx = match form.device.target {
                    TargetOs::Linux => 0,
                    TargetOs::Windows => 1,
                    TargetOs::MacOs => 2,
                    TargetOs::Bios => 3,
                };
                theme::segmented(
                    ui,
                    &mut seg_idx,
                    &["Linux", "Windows", "macOS", "BIOS / other"],
                );
                form.device.target = match seg_idx {
                    0 => TargetOs::Linux,
                    1 => TargetOs::Windows,
                    2 => TargetOs::MacOs,
                    _ => TargetOs::Bios,
                };

                ui.add_space(14.0);

                // Keychain checkbox
                ui.checkbox(&mut form.remember, "Remember password in the keychain");

                ui.add_space(16.0);

                // 1px rule
                let (rule_rect, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                ui.painter().hline(
                    rule_rect.x_range(),
                    rule_rect.center().y,
                    Stroke::new(1.0, theme::BORDER),
                );

                ui.add_space(16.0);

                // Actions: Cancel + Save and connect
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let valid = !form.device.host.trim().is_empty()
                            && !form.device.user.trim().is_empty();
                        save = ui
                            .add_enabled(
                                valid,
                                theme::primary("Save and connect").min_size(Vec2::new(150.0, 36.0)),
                            )
                            .clicked();
                        cancel = ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Cancel")
                                        .font(FontId::new(14.0, theme::bold_family()))
                                        .color(theme::TEXT),
                                )
                                .fill(theme::CARD_HOVER)
                                .stroke(Stroke::new(1.0, theme::BORDER))
                                .corner_radius(CornerRadius::same(6))
                                .min_size(Vec2::new(90.0, 36.0)),
                            )
                            .clicked();
                    });
                });
            });

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        let valid = !form.device.host.trim().is_empty() && !form.device.user.trim().is_empty();
        if valid && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            save = true;
        }

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

    /// Asks before deleting a saved device: its keychain password goes with it.
    fn remove_dialog(&mut self, ctx: &egui::Context) {
        let Some(i) = self.confirm_remove else { return };
        let Some(device) = self.store.devices.get(i) else {
            self.confirm_remove = None;
            return;
        };
        let name = device.display_name();
        let (mut confirm, mut cancel) = (false, false);
        egui::Modal::new(egui::Id::unique("remove-device"))
            .frame(theme::modal_frame())
            .backdrop_color(Color32::from_black_alpha(180))
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(theme::bold(format!("Remove {name}?"), 18.0).color(theme::TEXT));
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "The device and its saved password are deleted from this Mac.",
                    )
                    .size(13.0)
                    .color(theme::MUTED),
                );
                ui.add_space(16.0);
                let (rule_rect, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                ui.painter().hline(
                    rule_rect.x_range(),
                    rule_rect.center().y,
                    Stroke::new(1.0, theme::BORDER),
                );
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        confirm = ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Remove")
                                        .font(FontId::new(14.0, theme::bold_family()))
                                        .color(theme::BG),
                                )
                                .fill(theme::ERR)
                                .corner_radius(CornerRadius::same(6))
                                .min_size(Vec2::new(90.0, 36.0)),
                            )
                            .clicked();
                        cancel = ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Keep")
                                        .font(FontId::new(14.0, theme::bold_family()))
                                        .color(theme::TEXT),
                                )
                                .fill(theme::CARD_HOVER)
                                .stroke(Stroke::new(1.0, theme::BORDER))
                                .corner_radius(CornerRadius::same(6))
                                .min_size(Vec2::new(80.0, 36.0)),
                            )
                            .clicked();
                    });
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

    fn prompt_ui(&mut self, ctx: &egui::Context) -> Option<ConnectRequest> {
        let prompt = self.prompt.as_mut()?;
        let mut submit = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("prompt"))
            .frame(theme::modal_frame())
            .backdrop_color(Color32::from_black_alpha(180))
            .show(ctx, |ui| {
                ui.set_width(380.0);
                match prompt {
                    Prompt::Password {
                        device,
                        password,
                        remember,
                        wrong,
                    } => {
                        ui.label(
                            theme::bold(
                                format!("Password for {}@{}", device.user, device.host),
                                18.0,
                            )
                            .color(theme::TEXT),
                        );
                        if *wrong {
                            ui.add_space(8.0);
                            theme::banner(ui, theme::ERR, "The saved password was rejected.");
                        }
                        ui.add_space(12.0);
                        theme::field_label(ui, "Password");
                        let r = ui.add(
                            egui::TextEdit::singleline(password)
                                .password(true)
                                .desired_width(f32::INFINITY)
                                .min_size(Vec2::new(0.0, 36.0)),
                        );
                        r.request_focus();
                        submit = r.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter));
                        ui.add_space(12.0);
                        ui.checkbox(remember, "Remember in the keychain");
                        ui.add_space(14.0);
                        let (rule_rect, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 1.0),
                            Sense::hover(),
                        );
                        ui.painter().hline(
                            rule_rect.x_range(),
                            rule_rect.center().y,
                            Stroke::new(1.0, theme::BORDER),
                        );
                        ui.add_space(16.0);
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    submit |= ui
                                        .add(
                                            theme::primary("Connect")
                                                .min_size(Vec2::new(100.0, 36.0)),
                                        )
                                        .clicked();
                                    cancel = ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new("Cancel")
                                                    .font(FontId::new(14.0, theme::bold_family()))
                                                    .color(theme::TEXT),
                                            )
                                            .fill(theme::CARD_HOVER)
                                            .stroke(Stroke::new(1.0, theme::BORDER))
                                            .corner_radius(CornerRadius::same(6))
                                            .min_size(Vec2::new(80.0, 36.0)),
                                        )
                                        .clicked();
                                },
                            );
                        });
                    }
                    Prompt::Totp { device, code, .. } => {
                        ui.label(
                            theme::bold(format!("2FA code for {}", device.display_name()), 18.0)
                                .color(theme::TEXT),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(
                                "The device has two-factor authentication enabled.",
                            )
                            .size(13.0)
                            .color(theme::MUTED),
                        );
                        ui.add_space(12.0);
                        theme::field_label(ui, "Code");
                        let r = ui.add(
                            egui::TextEdit::singleline(code)
                                .char_limit(6)
                                .hint_text("123456")
                                .font(FontId::new(14.0, FontFamily::Monospace))
                                .desired_width(f32::INFINITY)
                                .min_size(Vec2::new(0.0, 36.0)),
                        );
                        r.request_focus();
                        submit = r.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter));
                        ui.add_space(14.0);
                        let (rule_rect, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 1.0),
                            Sense::hover(),
                        );
                        ui.painter().hline(
                            rule_rect.x_range(),
                            rule_rect.center().y,
                            Stroke::new(1.0, theme::BORDER),
                        );
                        ui.add_space(16.0);
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    submit |= ui
                                        .add(
                                            theme::primary("Connect")
                                                .min_size(Vec2::new(100.0, 36.0)),
                                        )
                                        .clicked();
                                    cancel = ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new("Cancel")
                                                    .font(FontId::new(14.0, theme::bold_family()))
                                                    .color(theme::TEXT),
                                            )
                                            .fill(theme::CARD_HOVER)
                                            .stroke(Stroke::new(1.0, theme::BORDER))
                                            .corner_radius(CornerRadius::same(6))
                                            .min_size(Vec2::new(80.0, 36.0)),
                                        )
                                        .clicked();
                                },
                            );
                        });
                    }
                }
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
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
