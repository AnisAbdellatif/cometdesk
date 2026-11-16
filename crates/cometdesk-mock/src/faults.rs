//! Fault injection for REST requests: delay, forced error statuses, dropped connections.
//! Entry points: `Faults`, `apply`. Read this to add another way for the device to misbehave.

use std::time::Duration;

use crate::State;
use crate::http::{Reply, Request};

#[derive(Default)]
pub(crate) struct Faults {
    pub delay: Duration,
    /// `(path, status, times left)`.
    pub fail: Vec<(String, u16, u32)>,
    /// Requests to swallow: close the connection without answering.
    pub drop_next: u32,
}

pub(crate) enum Fault {
    None,
    Drop,
    Reply(Reply),
}

pub(crate) async fn apply(state: &State, req: &Request) -> Fault {
    let (delay, fault) = {
        let mut f = state.faults.lock().unwrap();
        let fault = if f.drop_next > 0 {
            f.drop_next -= 1;
            Fault::Drop
        } else if let Some(i) = f.fail.iter().position(|(p, _, n)| *p == req.path && *n > 0) {
            f.fail[i].2 -= 1;
            Fault::Reply(Reply::error(f.fail[i].1, "InjectedError"))
        } else {
            Fault::None
        };
        (f.delay, fault)
    };
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    fault
}
