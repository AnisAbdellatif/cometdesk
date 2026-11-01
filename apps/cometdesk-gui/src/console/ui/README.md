# console/ui

- `mod.rs` — Per-frame egui rendering entry point (`Console::ui`) and toolbar visibility check.
- `status.rs` — Connection, stream, signal and HID status line computation.
- `toolbar.rs` — Top toolbar: device badge, status pill, Keys/Paste/Capture/Power menus and viewer controls.
- `panels.rs` — Target and Settings tab bodies inside the right-hand drawer.
- `overlays.rs` — Video canvas overlays: signal messages, region selection, fullscreen status, stats, typing and toasts.
- `drawer.rs` — Right-hand side drawer container and Target / Video / Settings tab switcher.
- `dialogs.rs` — Confirmation modal dialogs for typing, ATX power, OS session actions, bridge setup and video changes.
