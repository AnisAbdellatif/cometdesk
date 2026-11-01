//! Unit tests for fixture scrubbing and read-only fixture route invariants.

use serde_json::json;

use super::fixtures::{FIXTURE_ROUTES, scrub};

#[test]
fn scrubs_secrets_and_identifiers() {
    let mut v = json!({
        "system": {"platform": {"serial": "E7142E", "model": "v3"}},
        "turn": {"username": "u", "password": "p", "uris": ["turn:x"]},
        "devices": [{"mac": "aa:bb:cc:dd:ee:ff", "name": "nas"}],
        "available_devices": {"/dev/x": {"uuid": "005D-9024"}},
        "storage": {"images": {"my-secret-project.iso": {"size": 1}}},
        "drive": {"image": null},
    });
    scrub(&mut v);
    let s = v.to_string();
    for secret in [
        "E7142E",
        "\"u\"",
        "\"p\"",
        "aa:bb",
        "005D",
        "my-secret-project",
    ] {
        assert!(!s.contains(secret), "{secret} leaked: {s}");
    }
    assert_eq!(v["system"]["platform"]["model"], "v3");
    assert_eq!(v["storage"]["images"]["image-0.iso"]["size"], 1);
    assert!(v["drive"]["image"].is_null(), "null values stay null");
}

#[test]
fn fixture_routes_are_read_only_paths() {
    // Guard against someone adding a mutating GET.
    let forbidden = [
        "reboot",
        "reset",
        "delete",
        "create",
        "partition",
        "set_",
        "remove",
        "wake",
        "print",
    ];
    for (_, route, _) in FIXTURE_ROUTES {
        assert!(!forbidden.iter().any(|f| route.contains(f)), "{route}");
    }
}
