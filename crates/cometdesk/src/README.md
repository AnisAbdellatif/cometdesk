# cometdesk/src

- `lib.rs`: Crate root and top-level re-exports (`Session`, `Connection`, `Credentials`, `Error`).
- `api/`: Typed REST endpoints for ATX, bridge, capabilities, EDID, HID, info, MSD, streamer, system, and WoL.
- `client/`: Pre-login `Connection`, two-step login, and authenticated `Session` REST/WebSocket client.
- `env.rs`: `.env` loader and environment variable helpers for CLI and GUI.
- `error.rs`: SDK `Error` enum and `Result` alias.
- `events.rs`: `/api/ws` event stream and low-latency HID WebSocket sender.
- `hid.rs`: Binary HID wire protocol, key/mouse state coalescing, and wheel accumulation.
- `layout/`: Keyboard layout tables and character-to-HID-stroke translation.
- `shell.rs`: Remote root command execution over the device's webterm TTY.
- `tls.rs`: Trust-on-first-use (TOFU) TLS certificate store and verifier.
