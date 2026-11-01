//! Recording and scrubbing of read-only device API fixtures.
//!
//! Entry points: `record_fixtures`, `scrub`. Tests live in `tests.rs`.

use std::path::Path;

use anyhow::Result;
use cometdesk::Session;
use serde_json::{Value, json};

use super::util::with_streamer;

/// Read-only routes recorded as fixtures. Never add state-changing routes here: some of
/// them are GETs on this firmware (see `cometdesk::api`).
/// (fixture name, route, query)
type Route = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

pub(super) const FIXTURE_ROUTES: &[Route] = &[
    ("info", "info", &[]),
    ("upgrade_version", "upgrade/version", &[]),
    ("atx", "atx", &[]),
    ("wol_list", "wol/list", &[]),
    ("hid", "hid", &[]),
    ("hid_keymaps", "hid/keymaps", &[]),
    ("msd", "msd", &[]),
    ("system_capability", "system/capability", &[]),
    ("system_get_config", "system/get_config", &[]),
    ("2fa_is_enabled", "2fa/is_enabled", &[]),
];

/// Keys whose values identify a person or device, or are secrets.
const SCRUB_KEYS: &[&str] = &[
    "serial",
    "uuid",
    "mac",
    "token",
    "password",
    "username",
    "secret",
    "sn",
    "device_id",
    "ddns",
];

/// Replaces sensitive values in place; renames MSD image files.
pub fn scrub(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                if SCRUB_KEYS.contains(&k.as_str()) && !val.is_null() {
                    *val = json!("<scrubbed>");
                } else {
                    scrub(val);
                }
            }
            if let Some(Value::Object(images)) = map.get_mut("images") {
                let renamed = std::mem::take(images)
                    .into_iter()
                    .enumerate()
                    .map(|(i, (_, meta))| (format!("image-{i}.iso"), meta))
                    .collect();
                *images = renamed;
            }
        }
        Value::Array(items) => items.iter_mut().for_each(scrub),
        _ => {}
    }
}

pub async fn record_fixtures(session: &Session, dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let write = |name: &str, result: Value| -> Result<()> {
        let mut doc = json!({"ok": true, "result": result});
        scrub(&mut doc);
        let path = dir.join(format!("{name}.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&doc)? + "\n")?;
        println!("recorded {}", path.display());
        Ok(())
    };
    for (name, route, query) in FIXTURE_ROUTES {
        write(name, session.get::<Value>(route, query).await?)?;
    }
    write(
        "streamer_idle",
        session.get::<Value>("streamer", &[]).await?,
    )?;
    let running = with_streamer(session, async || {
        Ok(session.get::<Value>("streamer", &[]).await?)
    })
    .await?;
    write("streamer_running", running)?;
    Ok(())
}
