//! Typed REST APIs, one module per firmware API group (`kvmd/apps/kvmd/api/*.py`).
//!
//! Only routes the console needs are covered. Methods that change state say so; methods that
//! affect other viewers are marked **device-wide**. Response types use `#[serde(default)]`
//! throughout: a field missing on some firmware must never fail a whole response.
//!
//! Beware when adding routes: several state-changing routes on this firmware are `GET`s
//! (`upgrade/reboot`, `upgrade/reset_default`, `2fa/create`, `2fa/delete`,
//! `msd/partition_*`). Never call routes speculatively.

pub mod atx;
pub mod bridge;
pub mod capabilities;
pub mod hid;
pub mod info;
pub mod streamer;
pub mod system;
pub mod wol;

#[cfg(test)]
pub(crate) mod fixtures {
    //! Responses recorded from a real device (see `tests/fixtures/README.md`).

    use serde::de::DeserializeOwned;

    pub fn load<T: DeserializeOwned>(name: &str) -> T {
        let path = format!(
            "{}/tests/fixtures/rm1-v1.10.1/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let body = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        crate::client::parse_envelope(200, &body).unwrap_or_else(|e| panic!("{path}: {e}"))
    }
}
