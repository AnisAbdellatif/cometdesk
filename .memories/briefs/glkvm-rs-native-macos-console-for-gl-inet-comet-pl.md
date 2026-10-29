---
id: glkvm-rs-native-macos-console-for-gl-inet-comet-pl
title: "glkvm-rs: native macOS console for GL.iNet Comet, plus target-side bridge"
status: active
created: 2026-10-29T19:14:46.482Z
updated: 2026-10-29T19:14:46.482Z
tags:
  - glkvm
  - strategy
---

## Goal

A native, hardware-accelerated console for GL.iNet GLKVM devices (video, keyboard/mouse, power, audio) that beats the official web app on latency and power, built on a clean-room Rust SDK. Primary target is macOS on Apple Silicon. Reference device: GL-RM1 "Comet", firmware V1.10.1, kvmd 4.82.

Beyond the console, the work now covers a target-side channel between the Comet and the controlled machine:
- `glkvm-agent` runs on the target and talks to `glkvm-gui` through the Comet. It provides clipboard sync, text insertion, file send, power and notifications.
- A local JSON control socket (`tools/glkvmctl.py`) lets scripts and agents drive the GUI.

## Why Now

Phases 0 to 3 of `PLAN.md` have landed: Direct, Pion (FlexFEC-03) and Janus video, and Opus audio. M7 passes under injected loss. Work has moved to the clipboard bridge and agent features (`filesend`, `notify`, `power`, `receive`/`send` in `glkvm-clip`) and to automation (`docs/automation.md`). Much of this is still uncommitted.

## Constraints

- **Clean-room, MIT OR Apache-2.0.** GPL/AGPL sources (glkvm firmware, PiKVM, Overlook, RustDesk) are read for behavior only. No copied code or data files. `cargo-deny` enforces the dependency licenses.
- **No device-wide setting changes without an explicit user action.** Other viewers share the device. Video mode is stored server-side, and switching it breaks other viewers.
- **Undocumented firmware API.** Keep the API surface small and use capability detection. The Pion source depends on the patched `vendor/str0m` (see `vendor/str0m/GLKVM.md`).
- **Video decode uses VideoToolbox directly, with no ffmpeg.** Power is a first-class metric.
- **Clipboard transport.** USB serial and ECM are impossible on the Comet's kernel. RNDIS works but has side effects: no macOS targets, target routing changes, and the host must be on the Comet's LAN. The relay on the Comet (`glkvm-bridge`, a kvmd extra on nginx) removes those limits. `docs/clipboard-bridge.md` records the on-device findings.
- **Security.** The control socket gives full keyboard and mouse control, so it is off unless `GLKVM_CONTROL` is set, mode 0600, Unix only. Tokens stay out of logs. Auth tokens go in a Cookie header, never a query string.
- **Targets.** Linux and Windows for the RNDIS path. macOS targets need the relay path.

## Success Criteria

- Benchmarks M1 to M7 in `PLAN.md` §5.1 pass against the official web app, recorded in `bench/results.md`:
  - latency no worse than the web app
  - power at most 60 % of the web app's
  - 59 fps or better
  - first frame in 1.5 s with the streamer running
  - RSS under 200 MB
  - remote video at 30 fps or better over a lossy link
- Clipboard (text and files), text insertion, file send and power control work end to end through the Comet, on LAN and remote.
- Automation protocol is documented and scriptable.
- Pending work is committed with docs matching behavior.

## References

- `PLAN.md` (architecture, phases, risks, backlog)
- `docs/protocol.md`, `docs/clipboard-bridge.md`, `docs/automation.md`
- `graphify-out/GRAPH_REPORT.md`

## Status

Console MVP and remote video and audio are done. Clipboard bridge, agent features and automation are in progress and uncommitted.
