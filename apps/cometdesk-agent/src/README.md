# cometdesk-agent/src

- `main.rs`: Agent CLI entry point, single-instance lock, and reconnect loop.
- `session.rs`: Encrypted Noise session loop dispatching clipboard, file, capture, and control messages.
- `browse.rs`: Target directory listing handler for remote file browser.
- `capture.rs`: Target screen PNG capture per display.
- `clipboard.rs`: OS clipboard watcher and writer (text, HTML, image, file list).
- `config.rs`: Agent pairing state and persistent config storage.
- `display.rs`: Target display enumeration and resolution switching.
- `filesend.rs`: Incoming file transfer staging and atomic save.
- `info/`: Per-OS target telemetry (OS, CPU/RAM/disk load, battery, session state, focused app, keymap).
- `insert.rs`: Direct clipboard paste and keystroke injection on the target OS.
- `notify.rs`: Native desktop notification listener and forwarder.
- `power.rs`: Target OS lock, sleep, logout, reboot, and shutdown actions.
- `update.rs`: Self-update binary verification and replacement.
- `volume.rs`: Target OS master audio volume and mute control.
