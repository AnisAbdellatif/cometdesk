# Product

<!-- impeccable:product-schema 1 -->

## Platform

adaptive

## Users

Homelab and IT power users who own GL.iNet GLKVM devices (reference: GL-RM1 "Comet") and control headless or BIOS-level machines from a Mac. Sessions are often long, so latency and battery draw matter. Other viewers may share the same device.

## Product Purpose

CometDesk is a native console for GLKVM devices: video, keyboard/mouse and power control, built on a Rust SDK. It exists because the official web app costs more latency and power than a hardware-decoded native client needs to. Success is a session that feels local, runs cool on Apple Silicon, and never changes device-wide state without an explicit user action.

## Positioning

A native Rust/VideoToolbox client with zero-copy decode (IOSurface to Metal to wgpu) that beats the web app on latency and power, plus an optional on-target agent (clipboard sync, file send, text insert) that a pure KVM client cannot otherwise offer.

## Operating Context

- Targets are driven over HDMI out plus USB HID; nothing runs on the target unless the optional agent is installed.
- Typical flows: pick a saved device, open the console, send special keys, power/ATX/WoL, type or sync clipboard, fullscreen.
- Transports: Direct H.264 on LAN, Pion for remote; Janus only as fallback.
- Device passwords live in the OS keychain (Touch ID gated); auth tokens are memory-only.

## Capabilities and Constraints

- Today: macOS (Apple Silicon) only, egui on wgpu. Windows and Linux builds, a Settings Center and GL cloud are backlog; "Open device settings" opens the web UI.
- Platform is recorded as `adaptive` by the user's choice; no per-OS design languages exist yet, so macOS is the only shipped surface.
- Reactive repaint only; decode stops when minimized or occluded, stream drops after a 30 s grace period.
- Input v1: map mode (physical key position), per-device target OS profile, absolute mouse, special-keys menu.
- Licensing: MIT, clean-room; GPL/AGPL sources are read for behavior only.

## Evidence on Hand

PLAN.md (architecture and decisions), docs/protocol.md (captured protocol notes and CPU measurements), docs/clipboard-bridge.md, docs/automation.md, dist/CometDesk-0.1.0-arm64.dmg, apps/cometdesk-gui/assets/logo-256.png. No testimonials, customers or published benchmarks exist; do not invent them.

## Product Principles

1. Latency and power come first; every feature is weighed against decode cost and repaint frequency.
2. The console never silently changes shared device state.
3. Be honest about KVM limits: show HDMI signal and sleeping-target state rather than a mystery black screen.
4. Input must be predictable in BIOS and bootloaders: physical key position, no clever remapping that breaks there.
5. Security by default: secrets in the keychain, no tokens on disk.
