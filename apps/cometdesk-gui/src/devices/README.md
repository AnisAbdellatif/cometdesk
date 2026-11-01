# `devices` — Start screen and saved-device management

- `mod.rs` — Top-level `DevicesUi::ui` screen layout (header, footer, central panel, modals).
- `model.rs` — `DevicesUi`, `ConnectRequest`, `ConnectError`, form/prompt state, and keychain I/O.
- `reachability.rs` — Background TCP port-443 reachability probing with a 30 s cache TTL.
- `list.rs` — Empty-state card, saved-device grid, per-device card actions, and add-device tile.
- `form_modal.rs` — Centered modal dialog for adding or editing a saved device.
- `remove_dialog.rs` — Confirmation dialog before deleting a saved device and its keychain entry.
- `prompt.rs` — Password and TOTP 2FA modal prompts during connection.
