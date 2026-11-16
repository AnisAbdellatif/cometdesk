# Architecture

Crate dependencies (arrows = "depends on"):

    cometdesk-gui ─┬─> cometdesk-view ─> cometdesk-media ─> cometdesk (SDK)
                   ├─> cometdesk-media
                   ├─> cometdesk-clip   (no cometdesk deps; shared with the agent)
                   └─> cometdesk
    cometdesk-cli ──> cometdesk, cometdesk-media, cometdesk-view
    cometdesk-agent ─> cometdesk-clip     (runs on the target machine)

- `cometdesk`: SDK. Login/session, typed REST APIs, `/api/ws` events + HID path, TLS pinning,
  keyboard layouts for typing text over HID.
- `cometdesk-media`: video/audio transports (direct H.264/H.265 WS, Pion WebRTC, Janus), FlexFEC,
  VideoToolbox decode.
- `cometdesk-view`: frames to GPU (zero-copy), input mapping, keymap, QoS.
- `cometdesk-clip`: encrypted (Noise) protocol between GUI and target agent: clipboard, files,
  notifications, device info, updates.
- `cometdesk-gui`: egui/winit app. `app` owns window and events; `devices` the connect screen;
  `console` one live session; `clipsync` the background task talking to the agent; `theme` widgets.
- `cometdesk-mock`: dev-only fake GLKVM device (TLS, REST from recorded fixtures, `/api/ws`) used by SDK and CLI tests.

Each module directory has a `README.md` listing its files.
