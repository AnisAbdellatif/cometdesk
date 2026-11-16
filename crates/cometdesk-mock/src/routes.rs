//! REST routes of the mock device. Entry point: `route`. Read this to add or change an endpoint.

use std::sync::atomic::Ordering;

use crate::fixtures::result;
use crate::http::{Reply, Request};
use crate::{Call, PASSWORD, State, TOTP, USER};

fn field(body: &str, k: &str) -> String {
    url::form_urlencoded::parse(body.as_bytes())
        .find(|(key, _)| key == k)
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
}

pub(crate) fn streamer(state: &State) -> String {
    result(if state.viewers.load(Ordering::SeqCst) > 0 {
        "streamer_running"
    } else {
        "streamer_idle"
    })
}

pub(crate) fn atx(state: &State) -> String {
    if state.config.atx {
        r#"{"enabled":true,"busy":false,"leds":{"power":true,"hdd":false},"power":"on"}"#.into()
    } else {
        result("atx")
    }
}

pub fn route(state: &State, req: &Request) -> Reply {
    let (m, p) = (
        req.method.as_str(),
        req.path.strip_prefix("/api/").unwrap_or(""),
    );
    match (m, p) {
        ("GET", "2fa/is_enabled") => {
            return Reply::ok(format!(r#"{{"enabled":{}}}"#, state.config.twofa));
        }
        ("POST", "auth/login") => return login(state, req),
        ("POST", "auth/two_step_complete") => return two_step(state, req),
        _ => {}
    }
    if !state.authed(req) {
        return Reply::error(401, "UnauthorizedError");
    }
    state.calls.lock().unwrap().push(Call {
        method: req.method.clone(),
        path: req.path.clone(),
        query: req.query.clone(),
        body: req.body.clone(),
    });
    let atx_on = state.config.atx;
    match (m, p) {
        ("POST", "auth/logout") => {
            *state.valid_token.lock().unwrap() = None;
            Reply::ok("{}")
        }
        ("GET", "streamer") => Reply::ok(streamer(state)),
        ("POST", "streamer/set_params") => Reply::ok(streamer(state)),
        ("GET", "atx") => Reply::ok(atx(state)),
        ("GET", "info") => Reply::ok(result("info")),
        ("GET", "upgrade/version") => Reply::ok(result("upgrade_version")),
        ("GET", "system/capability") => Reply::ok(result("system_capability")),
        ("GET", "system/get_config") => Reply::ok(result("system_get_config")),
        ("GET", "hid") => Reply::ok(result("hid")),
        ("GET", "hid/keymaps") => Reply::ok(result("hid_keymaps")),
        ("GET", "msd") => Reply::ok(result("msd")),
        ("GET", "wol/list") => Reply::ok(result("wol_list")),
        ("POST", "atx/power" | "atx/click") if atx_on => Reply::ok("{}"),
        ("POST", "atx/power" | "atx/click") => Reply::error(400, "AtxDisabledError"),
        (
            "POST",
            "msd/set_params" | "msd/set_connected" | "msd/remove" | "hid/print" | "wol/wake"
            | "system/set_config",
        ) => Reply::ok("{}"),
        _ => Reply::error(404, "NotFoundError"),
    }
}

fn login(state: &State, req: &Request) -> Reply {
    state.logins.fetch_add(1, Ordering::SeqCst);
    let want = if state.config.twofa {
        format!("{PASSWORD}{TOTP}")
    } else {
        PASSWORD.into()
    };
    if field(&req.body, "user") != USER || field(&req.body, "passwd") != want {
        Reply::error(403, "ForbiddenError")
    } else if state.config.two_step {
        Reply::ok(r#"{"two_step_required":true,"two_step_token":"tst","expires_in":30}"#)
    } else {
        Reply::ok(format!(r#"{{"token":"{}"}}"#, state.issue_token()))
    }
}

fn two_step(state: &State, req: &Request) -> Reply {
    if field(&req.body, "two_step_token") != "tst" {
        Reply::error(403, "ForbiddenError")
    } else if state.polls.fetch_add(1, Ordering::SeqCst) < state.config.two_step_pending {
        Reply::ok(r#"{"status":"pending"}"#)
    } else {
        Reply::ok(format!(r#"{{"token":"{}"}}"#, state.issue_token()))
    }
}
