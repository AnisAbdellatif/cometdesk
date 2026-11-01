# `crates/cometdesk-cli/src/commands/`

- `mod.rs` — module declarations and `pub use` re-exports for all one-shot CLI commands.
- `util.rs` — shared `with_streamer` keep-alive helper and interactive `confirm` prompt.
- `info.rs` — `info` device summary and `snapshot` JPEG capture commands.
- `atx.rs` — `power` and `click` ATX power/button commands (`require_atx`).
- `video.rs` — `video_mode`, `venc_mode`, `latency_mode`, `codec`, `orientation`, and `quality` commands.
- `edid.rs` — `edid` display preset list and switch command.
- `msd.rs` — `msd_status`, `msd_upload`, `msd_mount`, `msd_unmount`, and `msd_remove` virtual media commands.
- `wol.rs` — `wake` and `wol_list` Wake-on-LAN commands.
- `type_text.rs` — `type_text` remote keystroke typing command.
- `fixtures.rs` — `record_fixtures` and `scrub` read-only API fixture recording.
- `bridge.rs` — `bridge_status`, `bridge_setup`, and `bridge_remove` clipboard bridge commands.
- `tests.rs` — unit tests for fixture secret scrubbing and read-only route invariants.
