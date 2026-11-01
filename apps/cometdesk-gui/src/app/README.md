# `app` — Window lifecycle, winit event loop, and local automation control

- `mod.rs` — `App` state, `UserEvent` enum, constructor, and graceful `shutdown`.
- `events.rs` — `ApplicationHandler<UserEvent>` implementation and keyboard/pointer event routing.
- `window.rs` — Redraw loop, input-capture predicates, fullscreen/title helpers, and shortcuts.
- `connect.rs` — Async device login and initial capability/config/keymap fetch (`start_connect`).
- `macos.rs` — AppKit fullscreen menu-bar hiding and unbundled Dock icon setup.
- `control.rs` — Opt-in Unix-socket JSON automation server (`COMETDESK_CONTROL`).
