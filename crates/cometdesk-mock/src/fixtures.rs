//! Recorded rm1-v1.10.1 device responses, embedded. Entry point: `result(name)`.
//! Read this to change which fixture a route serves.

const FILES: &[(&str, &str)] = &[
    (
        "atx",
        include_str!("../../cometdesk/tests/fixtures/rm1-v1.10.1/atx.json"),
    ),
    (
        "hid",
        include_str!("../../cometdesk/tests/fixtures/rm1-v1.10.1/hid.json"),
    ),
    (
        "hid_keymaps",
        include_str!("../../cometdesk/tests/fixtures/rm1-v1.10.1/hid_keymaps.json"),
    ),
    (
        "info",
        include_str!("../../cometdesk/tests/fixtures/rm1-v1.10.1/info.json"),
    ),
    (
        "msd",
        include_str!("../../cometdesk/tests/fixtures/rm1-v1.10.1/msd.json"),
    ),
    (
        "streamer_idle",
        include_str!("../../cometdesk/tests/fixtures/rm1-v1.10.1/streamer_idle.json"),
    ),
    (
        "streamer_running",
        include_str!("../../cometdesk/tests/fixtures/rm1-v1.10.1/streamer_running.json"),
    ),
    (
        "system_capability",
        include_str!("../../cometdesk/tests/fixtures/rm1-v1.10.1/system_capability.json"),
    ),
    (
        "system_get_config",
        include_str!("../../cometdesk/tests/fixtures/rm1-v1.10.1/system_get_config.json"),
    ),
    (
        "upgrade_version",
        include_str!("../../cometdesk/tests/fixtures/rm1-v1.10.1/upgrade_version.json"),
    ),
    (
        "wol_list",
        include_str!("../../cometdesk/tests/fixtures/rm1-v1.10.1/wol_list.json"),
    ),
];

/// The `result` field of fixture `name` as JSON text. Panics on an unknown name.
pub fn result(name: &str) -> String {
    let (_, raw) = FILES.iter().find(|(n, _)| *n == name).expect("fixture");
    let v: serde_json::Value = serde_json::from_str(raw).expect("fixture json");
    v["result"].to_string()
}
