//! Unit tests for target agent installer command generation.

use super::target::{agent_name, install_command};
use crate::store::TargetOs;

#[test]
fn install_commands_carry_the_code_and_fit_the_run_dialog() {
    let code = cometdesk_clip::Secret::generate().to_code();
    let linux = install_command(TargetOs::Linux, &code).unwrap();
    assert!(linux.contains(&format!("COMETDESK_CODE={code} sh \"$HOME/")));
    assert!(!linux.contains("/tmp/"));
    let win = install_command(TargetOs::Windows, &code).unwrap();
    assert!(win.contains(&code));
    // Typed over HID: no characters that need AltGr on common layouts.
    for cmd in [&linux, &win] {
        assert!(
            !cmd.contains(['|', '[', ']', '{', '}', '\\', '@', '~']),
            "{cmd}"
        );
    }
    // Windows' Run dialog accepts at most 259 characters.
    assert!(win.len() < 260, "{} chars", win.len());
    assert_eq!(install_command(TargetOs::MacOs, &code), None);
    assert_eq!(agent_name(TargetOs::Linux), Some("linux-x86_64"));
}
