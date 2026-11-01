//! Wake-on-LAN subcommands: wake a target by MAC or saved name, list saved targets.
//!
//! Entry points: `wake`, `wol_list`.

use anyhow::{Context, Result};
use cometdesk::Session;
use cometdesk::api::wol::Mac;

/// Wakes a MAC address or a saved target by name.
pub async fn wake(session: &Session, target: &str) -> Result<()> {
    let mac = match target.parse::<Mac>() {
        Ok(mac) => mac,
        Err(_) => {
            let devices = session.wol_list().await?;
            let found = devices
                .iter()
                .find(|d| d.name.eq_ignore_ascii_case(target))
                .with_context(|| {
                    format!("{target:?} is neither a MAC address nor a saved target")
                })?;
            found.mac.parse()?
        }
    };
    session.wol_wake(mac).await?;
    println!("magic packet sent to {mac}");
    Ok(())
}

pub async fn wol_list(session: &Session) -> Result<()> {
    let devices = session.wol_list().await?;
    if devices.is_empty() {
        println!("no saved targets");
    }
    for d in devices {
        println!("{:<20} {:<17} {}", d.name, d.mac, d.ip);
    }
    Ok(())
}
