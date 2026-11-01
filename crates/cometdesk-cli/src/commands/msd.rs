//! Virtual media (MSD) subcommands: status, image upload, mount, unmount, remove.
//!
//! Entry points: `msd_status`, `msd_upload`, `msd_mount`, `msd_unmount`, `msd_remove`.

use std::path::Path;

use anyhow::{Context, Result, bail};
use cometdesk::Session;

use super::util::confirm;

async fn require_msd(session: &Session) -> Result<cometdesk::api::msd::MsdState> {
    let state = session.msd_state().await?;
    if !state.enabled {
        bail!("virtual media is not enabled on this device");
    }
    Ok(state)
}

fn mib(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / (1 << 20) as f64)
}

pub async fn msd_status(session: &Session) -> Result<()> {
    let s = require_msd(session).await?;
    match (s.mounted(), &s.drive.image) {
        (Some(img), _) => println!(
            "mounted    {img} as {}",
            if s.drive.cdrom { "CD-ROM" } else { "USB drive" }
        ),
        (None, Some(img)) => println!("selected   {img} (not connected)"),
        (None, None) => println!("mounted    nothing"),
    }
    if let Some(free) = s.free() {
        println!("free       {}", mib(free));
    }
    for (name, img) in &s.storage.images {
        let note = if img.complete {
            ""
        } else {
            "  (incomplete upload)"
        };
        println!("{name:40} {:>12}{note}", mib(img.size));
    }
    Ok(())
}

pub async fn msd_upload(session: &Session, file: &Path, name: Option<String>) -> Result<()> {
    let state = require_msd(session).await?;
    let name = match name {
        Some(n) => n,
        None => file
            .file_name()
            .and_then(|n| n.to_str())
            .context("the file name isn't valid UTF-8; pass --name")?
            .to_owned(),
    };
    let size = std::fs::metadata(file)
        .with_context(|| format!("stat {}", file.display()))?
        .len();
    if state.storage.images.contains_key(&name) {
        bail!("an image named {name} already exists; remove it first or pass --name");
    }
    if state.free().is_some_and(|free| size > free) {
        bail!(
            "{} doesn't fit: {} free",
            mib(size),
            mib(state.free().unwrap())
        );
    }
    let last = std::sync::atomic::AtomicU64::new(0);
    session
        .msd_upload(file, &name, move |sent, total| {
            let pct = sent * 100 / total.max(1);
            if pct != last.swap(pct, std::sync::atomic::Ordering::Relaxed) {
                eprint!("\ruploading: {pct}%");
            }
        })
        .await?;
    eprintln!();
    println!("uploaded {name} ({})", mib(size));
    Ok(())
}

pub async fn msd_mount(session: &Session, image: &str, cdrom: bool, yes: bool) -> Result<()> {
    let state = require_msd(session).await?;
    if !state.storage.images.get(image).is_some_and(|i| i.complete) {
        bail!("no complete image named {image}; see `msd status`");
    }
    if state.drive.connected {
        bail!("a drive is already connected; run `msd unmount` first");
    }
    confirm(
        &format!(
            "Connect {image} to the target as a {}? (device-wide)",
            if cdrom { "CD-ROM" } else { "USB drive" }
        ),
        yes,
    )?;
    session.msd_mount(image, cdrom).await?;
    println!("mounted {image}");
    Ok(())
}

pub async fn msd_unmount(session: &Session, yes: bool) -> Result<()> {
    let state = require_msd(session).await?;
    if !state.drive.connected {
        println!("nothing is mounted");
        return Ok(());
    }
    confirm("Disconnect the drive from the target? (device-wide)", yes)?;
    session.msd_set_connected(false).await?;
    println!("unmounted");
    Ok(())
}

pub async fn msd_remove(session: &Session, image: &str, yes: bool) -> Result<()> {
    let state = require_msd(session).await?;
    if !state.storage.images.contains_key(image) {
        bail!("no image named {image}; see `msd status`");
    }
    if state.mounted() == Some(image) {
        bail!("{image} is mounted; run `msd unmount` first");
    }
    confirm(&format!("Delete {image} from the device?"), yes)?;
    session.msd_remove(image).await?;
    println!("removed {image}");
    Ok(())
}
