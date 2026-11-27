//! Mutable device state behind the write routes (streamer params, system config, MSD, WoL,
//! EDID), so a write is visible to the next read. Entry point: `Device::new`.
//! Read this to change what survives between requests.

use serde_json::{Map, Value, json};

use crate::fixtures::result;

pub(crate) struct Device {
    /// Overlay for `params` of the streamer state.
    pub streamer_params: Map<String, Value>,
    /// `system/get_config` result.
    pub config: Value,
    /// `msd` result.
    pub msd: Value,
    /// Saved Wake-on-LAN targets, `{mac, ip, name}`.
    pub wol: Vec<Value>,
    pub edid: String,
    /// ATX power LED (only reported when `Config::atx`).
    pub atx_on: bool,
}

pub(crate) const PRESETS: [(&str, &str, bool); 2] = [
    ("1080p60", "1920x1080/CometDesk/60Hz", true),
    ("720p60", "1280x720/CometDesk/60Hz", false),
];

/// A 128-byte EDID block as hex, distinguishable per preset by its first payload byte.
pub(crate) fn preset_edid(key: &str) -> String {
    let tag = if key == "1080p60" { "01" } else { "02" };
    format!("00ffffffffffff00{tag}{}", "00".repeat(119))
}

impl Device {
    pub fn new() -> Device {
        let parse = |n: &str| serde_json::from_str(&result(n)).unwrap();
        Device {
            streamer_params: Map::new(),
            config: parse("system_get_config"),
            msd: parse("msd"),
            wol: Vec::new(),
            edid: preset_edid("1080p60"),
            atx_on: true,
        }
    }

    pub fn presets() -> Value {
        PRESETS
            .iter()
            .map(|(key, label, default)| {
                json!({"key": key, "label": label, "content": preset_edid(key), "audio": false,
                       "hz": 60, "is_default": default})
            })
            .collect()
    }
}

/// Query/form values arrive as text; the device stores numbers and booleans as such.
pub(crate) fn coerce(v: &str) -> Value {
    match v {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => v
            .parse::<i64>()
            .map(Value::from)
            .or_else(|_| v.parse::<f64>().map(Value::from))
            .unwrap_or_else(|_| Value::String(v.into())),
    }
}
