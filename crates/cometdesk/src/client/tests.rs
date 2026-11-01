//! Unit tests for host normalization, TOTP password formatting, and envelope error parsing.
//! Entry points: exercised via `cargo test -p cometdesk`.
//! To change client or session logic, read `mod.rs` and `session.rs`.

use super::*;

#[test]
fn host_normalization() {
    assert_eq!(normalize_host("192.168.8.174"), "192.168.8.174");
    assert_eq!(
        normalize_host(" https://kvm.local:8443/ "),
        "kvm.local:8443"
    );
    assert_eq!(normalize_host("http://kvm"), "kvm");
}

#[test]
fn totp_code_is_appended_to_the_password() {
    assert_eq!(Credentials::new("admin", "pw").passwd(), "pw");
    assert_eq!(
        Credentials::new("admin", "pw")
            .with_totp(" 123456 ")
            .passwd(),
        "pw123456"
    );
}

#[test]
fn credentials_debug_hides_secrets() {
    let s = format!(
        "{:?}",
        Credentials::new("admin", "hunter2").with_totp("123456")
    );
    assert!(!s.contains("hunter2") && !s.contains("123456"), "{s}");
}

#[test]
fn parses_system_config_with_missing_and_extra_fields() {
    let json = r#"{"ok":true,"result":{"config":{"keymap":"de","scroll_rate":5,
        "reverse_scrolling":"STANDARD","theme_mode":"1","is_absolute_mouse":true}}}"#;
    let c = parse_envelope::<ConfigResult>(200, json.as_bytes())
        .unwrap()
        .config;
    assert_eq!(c.keymap.as_deref(), Some("de"));
    assert_eq!(c.scroll_rate, Some(5));
    assert_eq!(c.is_absolute_mouse, Some(true));
    assert_eq!(c.relative_sense, None);
}

#[test]
fn login_responses() {
    let ok = parse_login(
        200,
        br#"{"ok":true,"result":{"token":"t","failed_since_last_success":0}}"#,
    )
    .unwrap();
    assert_eq!(ok.token.as_deref(), Some("t"));
    let two = parse_login(
        200,
        br#"{"ok":true,"result":{"two_step_required":true,"two_step_token":"x","expires_in":60}}"#,
    )
    .unwrap();
    assert!(two.two_step_required);
    assert_eq!(
        (two.two_step_token.as_deref(), two.expires_in),
        (Some("x"), Some(60))
    );
    let pending = parse_login(200, br#"{"ok":true,"result":{"status":"pending"}}"#).unwrap();
    assert_eq!(pending.status.as_deref(), Some("pending"));
    assert!(matches!(
        parse_login(403, br#"{"ok":false,"result":{"error":"ForbiddenError"}}"#),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        parse_login(429, br#"{"ok":false,"result":{"error":"RateLimitError","error_msg":"wait"}}"#),
        Err(Error::RateLimited(m)) if m == "wait"
    ));
}

#[test]
fn envelope_errors() {
    assert!(matches!(
        parse_envelope::<serde_json::Value>(401, b""),
        Err(Error::Unauthorized)
    ));
    let unavailable =
        br#"{"ok":false,"result":{"error":"UnavailableError","error_msg":"Service Unavailable"}}"#;
    assert!(matches!(
        parse_envelope::<serde_json::Value>(503, unavailable),
        Err(Error::Api { status: 503, message }) if message == "Service Unavailable"
    ));
    assert!(matches!(
        parse_envelope::<serde_json::Value>(200, br#"{"ok":false,"result":{}}"#),
        Err(Error::Api { status: 200, .. })
    ));
}

#[test]
fn api_error_message_extraction() {
    let body = br#"{"ok":false,"result":{"error":"RateLimitError","error_msg":"slow down"}}"#;
    assert_eq!(api_message(body), "slow down");
    assert_eq!(
        api_message(b"<html>bad gateway</html>"),
        "<html>bad gateway</html>"
    );
}
