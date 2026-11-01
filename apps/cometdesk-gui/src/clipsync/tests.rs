//! Unit tests for agent version comparison and target restart detection.

use cometdesk_clip::Hello;

use super::types::{agent_is_older, restarted};

fn hello(protocol: u16, version: &str) -> Hello {
    Hello {
        protocol,
        version: version.into(),
        ..Hello::local("0", "t".into())
    }
}

#[test]
fn older_agents_are_updated_newer_ones_are_not() {
    let ours = env!("CARGO_PKG_VERSION");
    assert!(agent_is_older(&hello(cometdesk_clip::PROTOCOL - 1, ours)));
    assert!(!agent_is_older(&hello(
        cometdesk_clip::PROTOCOL + 1,
        "0.0.1"
    )));
    assert!(!agent_is_older(&hello(cometdesk_clip::PROTOCOL, ours)));
    assert!(agent_is_older(&hello(cometdesk_clip::PROTOCOL, "0.0.1")));
    assert!(!agent_is_older(&hello(cometdesk_clip::PROTOCOL, "99.0.0")));
}

#[test]
fn a_lower_uptime_means_a_restart() {
    assert!(restarted(Some(3600), 40));
    assert!(!restarted(Some(3600), 3610));
    assert!(!restarted(Some(100), 98));
    assert!(!restarted(None, 5));
}
