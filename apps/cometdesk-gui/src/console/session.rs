//! Session teardown, ATX power actions and agent RPC requests.
//! Entry points: `Console::disconnect`, `Console::atx`, `Console::session_action`, `Console::ask_agent`.

use cometdesk_clip::PowerAction as SessionAction;
use tokio::task::JoinHandle;

use super::Console;
use super::state::Confirm;
use crate::app::UserEvent;
use crate::clipsync::Request;

impl Console {
    pub(super) fn request_layout(&mut self, layout: String) {
        self.ask_agent(Request::Layout(layout), "Cannot switch the layout", None);
    }

    /// Sends `request` to the agent; `failure` prefixes the error shown if it says no, and
    /// `done` is shown if it says yes.
    pub(super) fn ask_agent(
        &mut self,
        request: Request,
        failure: &'static str,
        done: Option<String>,
    ) {
        let Some(rx) = self.clip.as_ref().map(|c| c.request(request)) else {
            return;
        };
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            let msg = match rx.await {
                Ok(Ok(())) => match done {
                    Some(m) => m,
                    None => return,
                },
                Ok(Err(e)) => format!("{failure}: {e}"),
                Err(_) => format!("{failure}: the agent disconnected"),
            };
            let _ = proxy.send_event(UserEvent::Notice(msg));
        });
    }

    pub(super) fn session_action(&mut self, action: SessionAction) {
        let Some(rx) = self
            .clip
            .as_ref()
            .map(|c| c.request(Request::Power(action)))
        else {
            return;
        };
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            let msg = match rx.await {
                Ok(Ok(())) => format!("{}: requested", action.label()),
                Ok(Err(e)) => format!("{} failed: {e}", action.label()),
                // The target went down before it could answer.
                Err(_) => format!("{}: the agent disconnected", action.label()),
            };
            let _ = proxy.send_event(UserEvent::Notice(msg));
        });
    }

    pub(super) fn atx(&mut self, confirm: Confirm) {
        let session = self.session.clone();
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            let (label, r) = match confirm {
                Confirm::Power(a) => (a.as_str(), session.atx_power(a, true).await),
                Confirm::Click(b) => (b.as_str(), session.atx_click(b, true).await),
                _ => return,
            };
            let msg = match r {
                Ok(()) => format!("ATX {label}: done"),
                Err(e) => format!("ATX {label} failed: {e}"),
            };
            let _ = proxy.send_event(UserEvent::Notice(msg));
        });
    }

    /// Tears the console down and logs out in the background; the returned handle completes
    /// when the logout does, so a quitting caller can wait for it.
    pub fn disconnect(mut self) -> JoinHandle<()> {
        self.release_all();
        if let Some(w) = self.watcher.take() {
            w.abort();
        }
        self.stream = None;
        self.clip = None;
        if let Some(t) = self.typing.take() {
            t.task.abort();
        }
        self.media.shutdown();
        let session = self.session.clone();
        let events = self.events.take();
        self.rt.spawn(async move {
            if let Some(e) = events {
                e.close().await;
            }
            let _ = session.logout().await;
        })
    }
}
