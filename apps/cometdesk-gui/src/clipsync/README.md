# `clipsync` — Clipboard & Target Agent Bridge

One-line guide to each file in this module:

- `mod.rs` — Module root, public re-exports, and macOS/fallback local clipboard polling (`LocalClipboard`, `pasteboard`).
- `bridge.rs` — Public `ClipBridge` handle (`spawn`, `state`, `settings`, `capture`, `send_file`, `take_notes`, `request`, `list_dir`, `insert`, `Drop`).
- `state.rs` — Connection state enum (`BridgeState`) and human-readable status labels.
- `settings.rs` — Clipboard sync direction/pause `Settings` and agent `Policy` conversion.
- `types.rs` — Request/result types, file transfer progress/batch state, notifications (`Note`), paste key mapping, and version/restart helpers.
- `supervise.rs` — Reconnection supervisor loop (`supervise`) and session termination reason (`End`).
- `run.rs` — Connected WebSocket session state (`Run`) and `tokio::select!` event loop (`run`).
- `files.rs` — Outgoing/incoming file transfer handlers, clipboard file batching, and piece-by-piece streaming.
- `insert.rs` — Text insertion request handler, paste keystroke reply handler, and insert timeout expiry.
- `capture.rs` — Target screenshot request handler, PNG/error reply handlers, and capture timeout expiry.
- `dirlist.rs` — Directory listing and target control/fetch ack handlers and timeout expiry.
- `notes.rs` — Handshake (`Ready`/`Mismatch`), target `Info`/restart alerts, `Notify` events, and clipboard sync handlers.
- `update.rs` — Queues an in-band `cometdesk-agent` binary update when the target runs an older version.
- `tests.rs` — Unit tests for agent version comparison and restart detection.
