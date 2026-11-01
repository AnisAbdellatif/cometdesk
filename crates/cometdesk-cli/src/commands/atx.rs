//! ATX power actions and front-panel button presses.
//!
//! Entry points: `power`, `click`.

use anyhow::{Result, bail};
use cometdesk::Session;
use cometdesk::api::atx::{AtxButton, PowerAction};

use super::util::confirm;

pub async fn power(session: &Session, action: PowerAction, yes: bool) -> Result<()> {
    require_atx(session).await?;
    confirm(
        &format!(
            "Send ATX power action \"{}\" to the target?",
            action.as_str()
        ),
        yes,
    )?;
    session.atx_power(action, true).await?;
    println!("done; power LED: {:?}", session.atx_state().await?.power);
    Ok(())
}

pub async fn click(session: &Session, button: AtxButton, yes: bool) -> Result<()> {
    require_atx(session).await?;
    confirm(
        &format!("Press the ATX \"{}\" button?", button.as_str()),
        yes,
    )?;
    session.atx_click(button, true).await?;
    println!("done");
    Ok(())
}

async fn require_atx(session: &Session) -> Result<()> {
    if !session.atx_state().await?.enabled {
        bail!("ATX is not enabled on this device (no ATX board configured)");
    }
    Ok(())
}
