//! Asynchronous session login and capability/config/keymap discovery on connect.
//!
//! Entry point: `App::start_connect`.

use std::sync::Arc;

use cometdesk::tls::PinStore;
use cometdesk::{Credentials, Error, Session};

use super::{App, UserEvent};
use crate::console::Connected;
use crate::devices::{ConnectError, ConnectRequest};

impl App {
    pub(super) fn start_connect(&mut self, req: ConnectRequest) {
        self.connecting = Some(format!("Connecting to {}…", req.device.display_name()));
        let proxy = self.proxy.clone();
        let pins = self.pins.clone();
        self.rt.spawn(async move {
            let result = connect(&req, pins).await.map_err(|e| (req, e));
            let _ = proxy.send_event(UserEvent::Connected(Box::new(result)));
        });
    }
}

async fn connect(req: &ConnectRequest, pins: Arc<PinStore>) -> Result<Connected, ConnectError> {
    let mut creds = Credentials::new(req.device.user.clone(), req.password.clone());
    creds.totp = req.totp.clone();
    let session = Session::login(&req.device.host, &creds, pins)
        .await
        .map_err(|e| match e {
            Error::Forbidden => ConnectError::WrongPassword,
            Error::TotpRequired => ConnectError::NeedTotp,
            e => ConnectError::Other(e.to_string()),
        })?;
    let (caps, config, keymaps) = tokio::join!(
        session.capabilities(),
        session.system_config(),
        session.hid_keymaps()
    );
    let caps = caps.map_err(|e| ConnectError::Other(e.to_string()))?;
    Ok(Connected {
        device: req.device.clone(),
        session,
        caps,
        config: config.unwrap_or_default(),
        keymaps: keymaps.map(|k| k.available).unwrap_or_default(),
    })
}
