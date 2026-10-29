//! Virtual media (`/api/msd`): disk images stored on the device, presented to the target as a
//! CD-ROM or a USB flash drive. Mounting is what the web app does: select the image and the
//! drive type (`set_params`), then connect the drive (`set_connected`).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use futures_util::stream;
use serde::Deserialize;
use tokio::io::AsyncReadExt;

use crate::{Error, Result, Session};

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct MsdState {
    pub enabled: bool,
    /// False while the device is switching the USB gadget; connecting then fails.
    pub online: bool,
    pub busy: bool,
    pub drive: MsdDrive,
    pub storage: MsdStorage,
}

/// The drive the target sees.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct MsdDrive {
    /// Selected image; absent until one was selected.
    pub image: Option<String>,
    /// CD-ROM when true, USB flash drive when false.
    pub cdrom: bool,
    pub connected: bool,
    /// Writable by the target (flash only).
    pub rw: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct MsdStorage {
    pub images: BTreeMap<String, MsdImage>,
    pub parts: BTreeMap<String, MsdPart>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct MsdImage {
    /// False for an upload that was interrupted.
    pub complete: bool,
    pub size: u64,
    /// Seconds since the epoch.
    pub mod_ts: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct MsdPart {
    pub free: u64,
    pub size: u64,
    pub writable: bool,
}

impl MsdState {
    /// Free space where images are stored (the root partition entry).
    pub fn free(&self) -> Option<u64> {
        self.storage.parts.values().map(|p| p.free).max()
    }

    /// True while the target sees the selected image.
    pub fn mounted(&self) -> Option<&str> {
        self.drive
            .connected
            .then_some(self.drive.image.as_deref())
            .flatten()
    }
}

fn flag(on: bool) -> &'static str {
    if on { "1" } else { "0" }
}

const CHUNK: usize = 1 << 20;

impl Session {
    pub async fn msd_state(&self) -> Result<MsdState> {
        self.get("msd", &[]).await
    }

    /// Selects the image and drive type. **Device-wide**; fails while a drive is connected.
    pub async fn msd_set_params(&self, image: &str, cdrom: bool) -> Result<()> {
        let _: serde_json::Value = self
            .post(
                "msd/set_params",
                &[("image", image), ("cdrom", flag(cdrom))],
            )
            .await?;
        Ok(())
    }

    /// Connects or disconnects the drive to the target. **Device-wide**.
    pub async fn msd_set_connected(&self, connected: bool) -> Result<()> {
        let _: serde_json::Value = self
            .post("msd/set_connected", &[("connected", flag(connected))])
            .await?;
        Ok(())
    }

    /// Selects `image` and connects it, like the web app's mount button.
    pub async fn msd_mount(&self, image: &str, cdrom: bool) -> Result<()> {
        self.msd_set_params(image, cdrom).await?;
        self.msd_set_connected(true).await
    }

    /// Deletes a stored image. Irreversible.
    pub async fn msd_remove(&self, image: &str) -> Result<()> {
        let _: serde_json::Value = self.post("msd/remove", &[("image", image)]).await?;
        Ok(())
    }

    /// Stores the file at `path` on the device as `image`, streaming it. An existing image of
    /// that name makes the device refuse. `progress(sent, total)` runs per chunk read.
    pub async fn msd_upload(
        &self,
        path: &Path,
        image: &str,
        progress: impl Fn(u64, u64) + Send + Sync + 'static,
    ) -> Result<()> {
        let file = tokio::fs::File::open(path).await?;
        let total = file.metadata().await?.len();
        let progress = Arc::new(progress);
        let body = stream::unfold((file, 0u64), move |(mut file, sent)| {
            let progress = progress.clone();
            async move {
                let mut buf = vec![0u8; CHUNK];
                let n = match file.read(&mut buf).await {
                    Ok(0) => return None,
                    Ok(n) => n,
                    Err(e) => return Some((Err(e), (file, sent))),
                };
                buf.truncate(n);
                let sent = sent + n as u64;
                progress(sent, total);
                Some((Ok::<_, std::io::Error>(buf), (file, sent)))
            }
        });
        let _: serde_json::Value = self
            .post_stream(
                "msd/write",
                &[("prefix", ""), ("image", image), ("remove_incomplete", "1")],
                reqwest::Body::wrap_stream(body),
            )
            .await
            .map_err(|e| match e {
                Error::Api { message, .. } if message.contains("Exists") => {
                    Error::Protocol(format!("an image named {image} already exists"))
                }
                e => e,
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::fixtures;

    #[test]
    fn parses_device_state() {
        let s: MsdState = fixtures::load("msd");
        assert!(s.enabled);
        assert!(!s.drive.connected);
        assert!(s.drive.cdrom);
        assert_eq!(s.drive.image, None);
        assert_eq!(s.mounted(), None);
        let img = &s.storage.images["image-0.iso"];
        assert!(img.complete);
        assert_eq!(img.size, 823132160);
        assert_eq!(s.free(), Some(5332107264));
    }

    #[test]
    fn mounted_needs_image_and_connection() {
        let mut s = MsdState::default();
        s.drive.image = Some("a.iso".into());
        assert_eq!(s.mounted(), None);
        s.drive.connected = true;
        assert_eq!(s.mounted(), Some("a.iso"));
    }
}
