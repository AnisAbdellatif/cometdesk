//! REST routes of the mock device. Entry point: `route`. Read this to add or change an endpoint.

use std::sync::atomic::Ordering;

use crate::fixtures::result;
use crate::http::{Reply, Request};
use crate::stateful::streamer;
use crate::{Call, PASSWORD, State, TOTP, USER};

fn field(body: &str, k: &str) -> String {
    url::form_urlencoded::parse(body.as_bytes())
        .find(|(key, _)| key == k)
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
}

pub(crate) fn atx(state: &State) -> String {
    if state.config.atx {
        r#"{"enabled":true,"busy":false,"leds":{"power":true,"hdd":false},"power":"on"}"#.into()
    } else {
        result("atx")
    }
}

pub fn route(state: &State, req: &Request) -> Reply {
    if req.method == "BAD" {
        return Reply::error(400, "BadRequestError");
    }
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
            if let Some(t) = req.token() {
                state.tokens.lock().unwrap().remove(t);
                let _ = state.push.send(crate::Push::CloseToken(t.into()));
            }
            Reply::ok("{}")
        }
        ("GET", "streamer") => Reply::ok(streamer(state)),
        ("GET", "atx") => Reply::ok(atx(state)),
        ("GET", "info") => Reply::ok(result("info")),
        ("GET", "upgrade/version") => Reply::ok(result("upgrade_version")),
        ("GET", "system/capability") => Reply::ok(result("system_capability")),
        ("GET", "hid") => Reply::ok(result("hid")),
        ("GET", "hid/keymaps") => Reply::ok(result("hid_keymaps")),
        ("POST", "atx/power" | "atx/click") if atx_on => Reply::ok("{}"),
        ("POST", "atx/power" | "atx/click") => Reply::error(400, "AtxDisabledError"),
        ("POST", "hid/print" | "wol/wake") => Reply::ok("{}"),
        _ => crate::stateful::route(state, m, p, req)
            .unwrap_or_else(|| Reply::error(404, "NotFoundError")),
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
