# cometdesk-gui/src

- `main.rs`: Binary entry point, Tokio runtime, and winit event loop startup.
- `actions.rs`: Keyboard shortcut definitions and matcher.
- `app/`: Top-level winit `ApplicationHandler`, window lifecycle, and local automation socket (`control.rs`).
- `browser.rs`: Target filesystem browser modal window.
- `clipsync/`: Background task supervising the Noise link to the target agent (`ClipBridge`).
- `console/`: Active KVM session view, toolbar, dropdown menus, command palette, and HID/media wiring.
- `crop.rs`: Sub-rectangle crop helper for `CVPixelBuffer` and RGBA screenshots.
- `devices/`: Saved device list, reachability probes, add/edit modal, and login/2FA/approval prompts.
- `gpu.rs`: wgpu instance, surface, device, and egui renderer state.
- `media.rs`: GUI wrapper around `cometdesk-media` video sources and VideoToolbox decoding.
- `ocr.rs`: macOS Vision framework OCR (`VNRecognizeTextRequest`) for screen text copy.
- `setup.rs`: First-run and target-agent setup wizard helpers.
- `store.rs`: Persistent JSON store and macOS Keychain credential storage.
- `sysnotify.rs`: macOS user notification helper.
- `theme/`: Colours, fonts, icons, and custom egui widgets.
- `touchid.rs`: macOS LocalAuthentication (Touch ID / Apple Watch) gate for sensitive actions.
