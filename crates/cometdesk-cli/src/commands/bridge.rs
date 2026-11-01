//! Clipboard bridge management subcommands: status, setup, remove.
//!
//! Entry points: `bridge_status`, `bridge_setup`, `bridge_remove`.

use std::path::Path;

use anyhow::{Context, Result, bail};
use cometdesk::Session;
use cometdesk::api::bridge;

pub async fn bridge_status(session: &Session) -> Result<()> {
    let status = session.bridge_status().await?;
    println!("health:   {:?}", bridge::health(status.as_ref()));
    let Some(st) = status else {
        println!("The bridge is not installed. Run: cometdesk bridge setup");
        return Ok(());
    };
    println!(
        "relay:    {} (this build installs {})",
        st.version,
        bridge::BRIDGE_VERSION
    );
    println!("firmware: {}", st.firmware.as_deref().unwrap_or("?"));
    if let Some(u) = &st.usb {
        println!(
            "usb:      gadget {} / interface {} / target link {}",
            yes_no(u.gadget),
            yes_no(u.iface),
            yes_no(u.carrier)
        );
    }
    for a in &st.agents {
        println!("agent:    {} ({} bytes, {:.12})", a.name, a.size, a.sha256);
    }
    println!(
        "session:  agent {}{}, client {}",
        if st.agent.connected {
            "connected"
        } else {
            "absent"
        },
        st.agent
            .peer
            .as_deref()
            .map(|p| format!(" ({p})"))
            .unwrap_or_default(),
        if st.client.connected {
            "connected"
        } else {
            "absent"
        },
    );
    Ok(())
}

fn yes_no(b: bool) -> &'static str {
    if b { "yes" } else { "NO" }
}

pub async fn bridge_setup(session: &Session, agents: &Path) -> Result<()> {
    let before = session.bridge_status().await?;
    println!("health before: {:?}", bridge::health(before.as_ref()));
    let report = bridge::setup(session, |msg| eprintln!("  {msg}")).await?;
    for c in &report.changes {
        println!("  {c}");
    }
    println!("relay {} installed", report.version);
    if report.usb_restarted {
        println!("(the target's USB devices were re-created: keyboard and mouse dropped briefly)");
    }
    let status = session
        .bridge_status()
        .await?
        .context("the bridge does not answer after setup")?;
    let uploaded = bridge::sync_agents(session, agents, &status).await?;
    if uploaded.is_empty() {
        println!("agent binaries: up to date");
    } else {
        println!("agent binaries uploaded: {}", uploaded.join(", "));
    }
    Ok(())
}

pub async fn bridge_remove(session: &Session, yes: bool) -> Result<()> {
    if !yes {
        bail!("this removes the bridge and restarts the device's USB functions; pass --yes");
    }
    bridge::remove(session).await?;
    println!("bridge removed");
    Ok(())
}
