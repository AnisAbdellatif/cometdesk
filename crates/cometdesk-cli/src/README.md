# `crates/cometdesk-cli/src/`

- `main.rs` — CLI binary entry point: environment/tracing init, credential loading, runtime setup, and top-level execution.
- `args.rs` — `clap` derive structs and enums (`Cli`, `Cmd`, `MsdCmd`, `BridgeCmd`, `*Arg`, `Toggle`, `parse_size`).
- `dispatch.rs` — session `login` (with TOTP prompt) and subcommand dispatch (`one_shot`, `view`, `probe`).
- `commands/` — one-shot subcommand implementations grouped by device feature (`info`, `atx`, `video`, `edid`, `msd`, `wol`, `type_text`, `bridge`, `fixtures`).
- `viewer.rs` — interactive `winit` console window, input event forwarding, and title status updates.
- `bench.rs` — `--bench` process CPU (`getrusage`) and per-frame latency measurement for the viewer.
- `renderer.rs` — `wgpu` window surface management wrapping `cometdesk_view::VideoRenderer`.
