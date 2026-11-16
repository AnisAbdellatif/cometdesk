//! Routes whose answer depends on earlier writes: streamer params, system config, MSD, WoL,
//! EDID, snapshot. Entry point: `route`. Read this to add a write-then-read endpoint.

use std::sync::atomic::Ordering;

use serde_json::{Value, json};

use crate::State;
use crate::device::{Device, coerce};
use crate::fixtures::result;
use crate::http::{Reply, Request};

static JPEG: &[u8] = include_bytes!("snapshot.jpg");

fn pairs(req: &Request) -> Vec<(String, String)> {
    url::form_urlencoded::parse(req.query.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

fn param(req: &Request, key: &str) -> String {
    pairs(req)
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
        .unwrap_or_default()
}

pub(crate) fn streamer(state: &State) -> String {
    let base = if state.viewers.load(Ordering::SeqCst) > 0 {
        "streamer_running"
    } else {
        "streamer_idle"
    };
    let mut v: Value = serde_json::from_str(&result(base)).unwrap();
    let dev = state.dev.lock().unwrap();
    for (k, val) in &dev.streamer_params {
        v["params"][k] = val.clone();
    }
    v.to_string()
}

/// `None` when the route is not handled here.
pub(crate) fn route(state: &State, m: &str, p: &str, req: &Request) -> Option<Reply> {
    let mut dev = state.dev.lock().unwrap();
    let images = |d: &Device| {
        d.msd["storage"]["images"]
            .as_object()
            .cloned()
            .unwrap_or_default()
    };
    Some(match (m, p) {
        ("POST", "streamer/set_params") => {
            for (k, v) in pairs(req) {
                dev.streamer_params.insert(k, coerce(&v));
            }
            drop(dev);
            Reply::ok(streamer(state))
        }
        ("GET", "streamer/snapshot") => {
            if state.viewers.load(Ordering::SeqCst) == 0 && param(req, "allow_offline") != "1" {
                Reply::error(503, "UnavailableError")
            } else {
                Reply::jpeg(JPEG)
            }
        }
        ("GET", "system/get_config") => Reply::ok(dev.config.to_string()),
        ("POST", "system/set_config") => match serde_json::from_str::<Value>(&req.body) {
            Ok(v) if v.is_object() => {
                dev.config["config"] = v;
                Reply::ok("{}")
            }
            _ => Reply::error(400, "ValidatorError"),
        },
        ("GET", "msd") => Reply::ok(dev.msd.to_string()),
        ("POST", "msd/set_params") => {
            let image = param(req, "image");
            if !image.is_empty() && !images(&dev).contains_key(&image) {
                return Some(Reply::error(400, "MsdUnknownImageError"));
            }
            dev.msd["drive"]["image"] = if image.is_empty() {
                Value::Null
            } else {
                image.into()
            };
            dev.msd["drive"]["cdrom"] = (param(req, "cdrom") != "0").into();
            Reply::ok("{}")
        }
        ("POST", "msd/set_connected") => {
            let on = param(req, "connected") == "1";
            if on && dev.msd["drive"]["image"].is_null() {
                return Some(Reply::error(400, "MsdImageNotSelectedError"));
            }
            dev.msd["drive"]["connected"] = on.into();
            Reply::ok("{}")
        }
        ("POST", "msd/remove") => {
            let image = param(req, "image");
            let removed = dev.msd["storage"]["images"]
                .as_object_mut()
                .is_some_and(|m| m.remove(&image).is_some());
            if removed {
                Reply::ok("{}")
            } else {
                Reply::error(400, "MsdUnknownImageError")
            }
        }
        ("POST", "msd/write") => {
            let image = param(req, "image");
            if images(&dev).contains_key(&image) {
                return Some(Reply::error(400, "MsdImageExistsError"));
            }
            dev.msd["storage"]["images"][&image] =
                json!({"complete": true, "mod_ts": 0, "removable": true, "size": req.body_len});
            Reply::ok("{}")
        }
        ("GET", "wol/list") => Reply::ok(json!({"devices": dev.wol}).to_string()),
        ("POST", "wol/add") => {
            let mac = param(req, "mac").to_ascii_lowercase();
            dev.wol.retain(|d| d["mac"] != mac.as_str());
            dev.wol
                .push(json!({"mac": mac, "ip": param(req, "ip"), "name": param(req, "name")}));
            Reply::ok("{}")
        }
        ("POST", "wol/remove") => {
            let mac = param(req, "mac").to_ascii_lowercase();
            dev.wol.retain(|d| d["mac"] != mac.as_str());
            Reply::ok("{}")
        }
        ("GET", "upgrade/get_edid") => Reply::ok(json!({"edid": dev.edid}).to_string()),
        ("GET", "upgrade/edid_list") => Reply::bare(Device::presets().to_string()),
        ("POST", "upgrade/edid") => {
            let hex = multipart_field(&req.body, "edid").unwrap_or_default();
            if hex.len() < 2
                || !hex.len().is_multiple_of(2)
                || !hex.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Some(Reply::error(400, "ValidatorError"));
            }
            dev.edid = hex.to_ascii_lowercase();
            Reply::ok("{}")
        }
        _ => return None,
    })
}

/// The text of form field `name` in a `multipart/form-data` body.
fn multipart_field(body: &str, name: &str) -> Option<String> {
    let start = body.find(&format!("name=\"{name}\""))?;
    let rest = &body[start..];
    let rest = &rest[rest.find("\r\n\r\n")? + 4..];
    Some(rest[..rest.find("\r\n--")?].trim().to_owned())
}
