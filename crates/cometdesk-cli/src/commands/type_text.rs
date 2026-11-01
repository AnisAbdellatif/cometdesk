//! Remote keystroke typing (`hid/print`) subcommand.
//!
//! Entry point: `type_text`.

use std::io::Read;

use anyhow::{Result, bail};
use cometdesk::Session;
use cometdesk::api::hid::PrintOptions;

pub async fn type_text(
    session: &Session,
    text: Option<String>,
    keymap: Option<&str>,
    slow: bool,
) -> Result<()> {
    let text = match text {
        Some(t) => t,
        None => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            buf
        }
    };
    if text.is_empty() {
        bail!("nothing to type");
    }
    session
        .hid_print(&text, PrintOptions { keymap, slow })
        .await?;
    println!("typed {} characters", text.chars().count());
    Ok(())
}
