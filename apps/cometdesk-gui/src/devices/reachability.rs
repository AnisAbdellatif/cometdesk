//! Host TCP reachability probing with a 30-second cache TTL.
//!
//! Entry points: `DevicesUi::probe_hosts`, `DevicesUi::reachability`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::DevicesUi;

pub(super) type Probes = Arc<Mutex<HashMap<String, (Instant, Option<bool>)>>>;

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
    pub(super) fn probe_hosts(&self, ctx: &egui::Context) {
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

    pub(super) fn reachability(&self, host: &str) -> Option<bool> {
        self.probes.lock().unwrap().get(host).and_then(|(_, a)| *a)
    }
}
