//! Display EDID preset listing and selection subcommand.
//!
//! Entry point: `edid`.

use anyhow::{Result, bail};
use cometdesk::Session;

use super::util::confirm;

pub async fn edid(session: &Session, key: Option<&str>, yes: bool) -> Result<()> {
    let current = session.edid().await?;
    let presets = session.edid_presets().await?;
    let is_current = |p: &cometdesk::api::edid::EdidPreset| {
        cometdesk::api::edid::normalize(&p.content) == current
    };
    let Some(key) = key else {
        for p in &presets {
            println!(
                "{} {:<18} {}{}",
                if is_current(p) { "*" } else { " " },
                p.key,
                p.label,
                if p.audio { "" } else { " (no audio)" }
            );
        }
        if !presets.iter().any(is_current) {
            println!("* (custom EDID)");
        }
        return Ok(());
    };
    let Some(p) = presets.iter().find(|p| p.key == key) else {
        bail!("unknown EDID preset {key:?}; run `edid` to list them");
    };
    if is_current(p) {
        println!("already {}", p.label);
        return Ok(());
    }
    confirm(
        &format!(
            "Present the target a {} display? The target sees a monitor change. \
             This is device-wide and persisted.",
            p.label
        ),
        yes,
    )?;
    session.set_edid(&p.content).await?;
    println!("edid: {} (confirmed)", p.label);
    Ok(())
}
