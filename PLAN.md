# glkvm-rs: native console for GL.iNet GLKVM devices

Goal: a native, hardware-accelerated **console** for GLKVM devices (video + keyboard/mouse
+ power) that beats the official web app on latency and power, built on a Rust SDK that
covers what the console needs. Primary target is macOS (Apple Silicon).

Non-goals (for now): reimplementing the web app's Settings Center, GL cloud, Windows and
Linux builds. They are in the backlog (§8) and get pulled in only when a concrete need
appears. Until then, the GUI has an **"Open device settings"** button that opens the web UI.

Reference device: GL-RM1 "Comet", firmware V1.10.1, kvmd 4.82, at `192.168.8.174`.

## 0. Decisions

| Decision | Choice | Why |
|---|---|---|
| License | **MIT OR Apache-2.0**, clean-room | Lets anyone embed the SDK. Possible because nothing below needs GPL code or data. |
| Clean-room rule | GPL/AGPL/unlicensed sources (glkvm firmware, PiKVM, Overlook, RustDesk, hwcodec) are **read for behavior only**. No code, no data files (incl. `keymap.csv`), no translated JS. | Implementing a network protocol from observed behavior is fine; copying is not. |
| Key code table | Hand-written from the W3C UI Events `KeyboardEvent.code` spec and the USB HID Usage Tables | Public specs; maps 1:1 to `winit::keyboard::KeyCode`. |
| Dependency licenses | `cargo-deny` in CI, deny GPL/AGPL/LGPL and unlicensed crates | Keeps the license decision enforceable. |
| Video decode | VideoToolbox directly (`objc2-video-toolbox`). **No ffmpeg.** | The go/no-go is about power, so the spike must use hardware decode anyway; ffmpeg adds build pain and LGPL questions for nothing. |
| Primary transport | Direct H.264 (`/api/media/ws`) on LAN; Pion (`/pion/ws`) without FEC for remote | Fewest moving parts. Janus only if Pion fails (§5, Phase 3). |
| Device-wide state | The client never changes device-wide settings without an explicit user action (§4) | Other viewers share the device. |

## 1. What we are talking to

The device runs **GLKVM KVMD**, GL.iNet's GPLv3 fork of PiKVM
([gl-inet/glkvm](https://github.com/gl-inet/glkvm)). Everything goes through nginx on
HTTPS (self-signed cert by default):

| Surface | Endpoint | Purpose | Used by us |
|---|---|---|---|
| REST API | `/api/...` (231 routes, see Appendix A) | Auth, settings, MSD, ATX, network, VPN, upgrade, ... | Subset (§2.1) |
| Event socket | `wss://host/api/ws` | Pushed state events (`*_state`, `kickout`, `pong`) + HID input (JSON and binary) | Phase 1 |
| Video: Direct | `wss://host/api/media/ws` | Raw H.264 over WebSocket. No audio. | Phase 0 |
| Video: WebRTC(FEC) | `wss://host/pion/ws` | GL's Pion server, FlexFEC-03, audio, camera, HID over data channel. Device default. | Phase 3 |
| Video: WebRTC | `wss://host/janus/ws` | Classic PiKVM Janus + uStreamer plugin. Video, audio, mic. | Fallback only |
| Video: WebRTC(Native) | `wss://host/lws/ws` | GL's native client ("adaptive mode"). **Kills Janus + uStreamer** while active. | No |
| Serial console | `wss://host/api/serial/ws` | Serial terminal | Backlog |
| HID relay | `/api/hid/ws` | Internal, for gl-pion only (`allowed_exe_paths`) | No |

Auth: `POST /api/auth/login` → `auth_token` cookie. 2FA via `/api/auth/login_with_2fa`,
`/api/2fa/*`; two-step approval flow via `/api/auth/two_step_*`. On WebSockets we send the
token as a `Cookie` header on the upgrade request, not as `?auth_token=` (query strings
end up in nginx logs).

Things learned the hard way in this investigation:

- The selected video mode is stored server-side (`/api/system/get_config` →
  `video_mode`: `flex_fec | webrtc | gl_webrtc | direct_h264`). Mode switching is
  done with `POST /api/streamer/set_params?gl_webrtc=true|false`. While
  `gl_webrtc=true`, `/janus/ws` returns **502**.
- The streamer is on demand: `/api/streamer` returns `streamer: null` when no viewer
  is connected. Opening `/api/ws` (without `stream=0`) is what keeps it alive.
- `/api/streamer` reports `hdmi.signal` / `source.online`: "connected but black" is
  often just a sleeping target. Surface this in the UI.

### 1.1 Open protocol questions (answer in Phase 0, before writing the SDK)

Each one gets a short note in `docs/protocol.md` with a captured example.

| # | Question | Why it matters |
|---|---|---|
| U1 | Binary HID frames on `/api/ws`. Overlook's working encoder (§10) uses a leading type byte: key `[0x01][state][key name]`, button `[0x02][state][left\|right\|middle]`, abs move `[0x03][x i16 BE][y i16 BE]`, relative `[0x04][squash][dx i8][dy i8]`, wheel `[0x05][squash][dx i8][dy i8]`. Still to confirm on the device: the `finish` flag (bit1 of state, per upstream) and touch (`0x06`). | The HID encoder. |
| U2 | `/api/media/ws` binary frames: Annex B or AVCC? One access unit per message? Any timestamp? When are SPS/PPS sent? | VT needs AVCC + a format description. |
| U3 | Does `/api/media/ws` keep the streamer alive on its own, or must `/api/ws` stay open too? | Connection lifecycle. |
| U4 | Does Direct mode work while `gl_webrtc=true`? Does Pion? | Whether we ever need to touch the saved mode. |
| U5 | Does Direct mode ever carry H.265 (`features.h265 = true`)? | Whether H.265 is a v1 concern at all. |
| U6 | Switching mouse to relative: Overlook does it via `system/set_config` → `is_absolute_mouse`, not `hid/set_params`. Does that re-enumerate USB on the target, and does it affect other viewers? | Relative mode UX, §4. |
| U7 | Do the routes in the `gl-inet/glkvm` source match what V1.10.1 serves on the device? (Appendix A must be checked against the live device.) | GPL source drops can lag firmware releases. |
| U8 | `POST hid/set_connected?connected=true\|false`: Overlook calls it on every connect/disconnect. What is the default, is it ever needed for HID to work, and does `false` cut HID for every viewer? | Whether we touch it at all (§4). |

Whether gl-pion accepts plain RTP without FlexFEC is answered at the start of Phase 3.

## 2. Architecture

Cargo workspace, async on `tokio`, all TLS via `rustls`. Four crates; split further only
when compile times or a second consumer demand it.

```
glkvm-rs/
  crates/
    glkvm/          SDK: proto types (serde), REST client, /api/ws events + HID. Light deps.
    glkvm-media/    Video/audio sources (Direct, Pion), decoders (VideoToolbox). Heavy deps.
    glkvm-cli/      `glkvm` CLI: dogfoods the SDK (view, snapshot, power, wake, type).
  apps/
    glkvm-gui/      eframe/egui desktop app.
```

### 2.1 `glkvm::client` (REST)

- `reqwest` with cookie store, rustls, gzip.
- **TLS**: one shared `rustls::ClientConfig` with a trust-on-first-use verifier (pin the
  device cert's SHA-256), used by both `reqwest` and `tokio-tungstenite`. On mismatch, a
  typed `CertChanged { old, new }` error; the GUI shows both fingerprints and offers to
  re-pin (expected after a factory reset or an SSL cert upload).
- `Device::connect(host, Credentials) -> Session`; re-login on 401 using credentials from
  the keychain; 2FA and two-step approval are explicit states, not errors.
- Capability gating: `/api/info`, `/api/system/capability`, `/api/system/otg_functions`
  and the model (RM1, RM1PE, RM10, RM10C4, RMQ1, RM4PE) decide which features exist.
  Expose `device.supports(Feature::Atx)` rather than letting calls 404.
- **Coverage is driven by the console, not by the route list.** v1 modules:
  `auth` (incl. 2FA, two-step), `info`, `system` (capability, clients, get_config only),
  `streamer` (state, snapshot), `hid` (print, keymaps), `atx`, `wol`. Everything else is
  added when a backlog item needs it.
- Every setter that changes device-wide or persisted state is marked `#[doc = "Device-wide"]`
  and lives behind an explicit method name (`set_device_video_mode`, never a side effect of
  something else). See §4.

### 2.2 `glkvm::events` (`/api/ws`)

- `tokio-tungstenite`. Typed `enum Event { Info, Hid, Atx, Streamer, Kickout, ... }`
  (unknown event types are kept as `Event::Other(serde_json::Value)`, not errors).
- Two outputs: a `watch::Receiver<DeviceState>` with the latest full state (what the GUI
  reads), and a `broadcast` of events for consumers that want every change. A lagging
  broadcast receiver loses events but never state, because state is in the `watch`.
- HID send path uses the binary frames, format per U1. Key codes are W3C
  `KeyboardEvent.code` strings (`KeyA`, `ShiftLeft`, ...) from our own table (§0),
  including the keys Overlook misses: `IntlBackslash` (ISO), JIS keys, `Insert`,
  `PrintScreen`, `F13`–`F24`.
- **One ordered HID queue** per session (a single task owns the socket writer). Never
  spawn a task per event: key down/up reordering produces stuck or doubled keys.
- **Coalescing** inside that queue:
  - absolute moves: latest wins, flushed at most every 8.33 ms (120 Hz), idle when nothing moves;
  - relative moves: summed, then split into `i8` frames (±127);
  - wheel: accumulate fractional deltas (trackpads send many small precise deltas), emit
    whole ticks only, scaled by `scroll_rate` and direction by `reverse_scrolling`
    (both read from `system/get_config`, see §4);
  - a button press is preceded by an absolute move to the click point.
- **Coordinates**: the SDK takes normalized video coordinates (`0.0..=1.0`, origin top-left
  of the video as displayed), applies the device `orientation`, and scales to ±32767.
  Points outside the video are dropped, not clamped to the edge. Mapping window → video
  rectangle (letterboxing, HiDPI) is the GUI's job.
- `release_all()`: sends key-up for every key and button the session believes is held.
  Called on window focus loss, capture toggle, socket loss and disconnect.
- Keepalive: JSON `{"event_type":"ping","event":{}}` every 2 s; missing `pong` for 3
  intervals → reconnect with exponential backoff (Overlook has none and silently loses HID).
  `kickout` → `SessionEnded` state.
- Pass `stream=0` when the session only needs HID and events (Overlook's flag is ignored,
  so its HID socket keeps the streamer alive).

### 2.3 `glkvm-media` (sources)

Sources are an enum, not a trait object (`async fn` in traits isn't dyn-compatible, and
there are only two):

```rust
pub enum SourceKind { Direct, Pion }

pub async fn start(session: &Session, kind: SourceKind, opts: StreamOpts) -> Result<MediaSession>;

pub struct MediaSession {
    pub video: mpsc::Receiver<EncodedFrame>,
    pub audio: Option<mpsc::Receiver<OpusPacket>>,
    pub stats: watch::Receiver<MediaStats>,   // fps, bitrate, rtt, loss, decode time
    pub control: MediaControl,                // request_keyframe(), stop()
}

pub struct EncodedFrame {
    pub codec: Codec,          // H264 (H265 only if U5 says so)
    pub data: Bytes,           // Annex B access unit
    pub keyframe: bool,
    pub pts: Option<Duration>,
    pub received_at: Instant,  // for latency stats
}
```

Implementations:

1. **Direct** (`/api/media/ws`): JSON `media` event gives `h264.profile_level_id`, client
   sends `start {type: video, format: h264}`, binary frames follow, `[0]` requests a key
   frame. Present on arrival, no jitter buffer (LAN). TCP means head-of-line blocking on
   lossy links, which is why remote uses Pion.
2. **Pion** (`/pion/ws`, Phase 3): client is offerer; first message
   `{type: config, video, speaker, mic, hid, offerer: true, flexfecLevel, orientation,
   camera}`, then offer/answer/candidate JSON. WebRTC via
   **[str0m](https://github.com/algesten/str0m)** (sans-IO, H.264 + Opus, NACK/RTX). str0m
   has no FlexFEC, so the FlexFEC-03 repair stream is decoded by us (§2.3 status; a small
   patch of str0m hands over the packets). gl-pion accepts plain RTP but only FEC survives loss.
3. **Janus** (`/janus/ws`): only if Pion fails. Requires `gl_webrtc=false`, which is a
   device-wide change (§4). Signaling details are in §10 (the `janus-protocol`
   subprotocol and `Origin` header are required; Janus is the offerer).

Rules for every source:

- **Video and audio in separate sessions.** Overlook runs audio on a second Janus handle and
  PeerConnection, because sharing one makes the receiver hold video back for A/V sync. Same
  for Pion: request video only on the main connection, and audio on a second one (or skip
  audio).
- **No jitter buffer beyond reordering.** Present on arrival (Overlook forces libwebrtc's
  playout delay to 0 and re-applies it every 2 s because libwebrtc resets it). With str0m we
  own this: hold a frame only until it is complete, never for smoothing.
- **Stall watchdog**: no first frame within 5 s of connecting, or no frame for 3 s → request a
  keyframe once, then reconnect with exponential backoff. Surface the state in the UI.
- **Signaling hygiene**: every request has a timeout, and error replies are parsed into typed
  errors rather than surfacing as "connection lost".
- **ICE for remote**: get TURN credentials from `GET /api/turn/get_turn`
  (`{username, password, ttl, uris}`), refreshed before `ttl` expires. No hardcoded public
  STUN.

`/lws/ws` (Native) is out of scope: proprietary, and it disables Janus.

**Status (branch `phase3-webrtc`)**: all three sources exist behind one switch,
`glkvm_media::start(session, SourceOptions { transport, .. })` with `Transport::{Direct,
Pion, Janus}` (CLI: `view --transport direct|pion|janus`; GUI: `GLKVM_TRANSPORT`). Pion and
Janus share a str0m engine (`rtc.rs`): H.264 only, host candidates only (str0m has no
STUN/TURN, so the device must be reachable over UDP: LAN or VPN), NACK/RTX, PLI for keyframes.
Pion also receives **FlexFEC-03**: the offer is munged like the web app's, repair packets reach
`flexfec.rs` through a small patch of str0m (`vendor/str0m/GLKVM.md`), and rebuilt packets are
injected into the receive path. gl-pion's repair layout is documented in `docs/protocol.md`.
Verified against the Comet over Tailscale on 2026-10-29: handshakes work (Pion SDPs are base64
JSON), every group XORs to zero. Without FEC the stream does not survive loss (gl-pion sends
unpaced bursts and never answers NACK); Janus mode has no FEC, so Direct stays the choice for lossy links
in `webrtc` mode. **M7** (`pion_video_survives_loss`, 1 % of incoming RTP dropped in-process,
`GLKVM_TEST_RX_LOSS=1`; dummynet needs root): see `bench/results.md`. On unrecovered loss the
source keeps decoding (artifacts until the next IDR) rather than freezing, because the device
sends keyframes rarely on a static screen. TURN credentials (`/api/turn/get_turn`) are not
used: str0m has no STUN/TURN client, so relayed paths are out of scope.
Audio: `pion::start_audio` or `janus::start_audio` opens a second, audio-only connection (`speaker: true`,
Opus recv-only); `audio::Player` decodes with the pure-Rust `opus-decoder` (no libopus/cmake)
and plays through `cpal` with a 40 ms prebuffer and a 150 ms latency cap. CLI: `view --transport
pion --audio`; GUI: Video menu → "Play audio". Unit-tested for the decoder glue and buffer
behavior. On the device (2026-10-29) both Pion and Janus audio deliver Opus once the target
plays sound, and decode and reach the output callback; the source follows the device's video
mode (`audio::start`).

### 2.4 Decode

```rust
pub trait Decoder {
    fn push(&mut self, frame: EncodedFrame) -> Result<(), DecodeError>;
    fn poll(&mut self) -> Option<DecodedFrame>;   // 0..n frames per push
    fn flush(&mut self);
}

pub enum DecodedFrame {
    #[cfg(target_os = "macos")]
    PixelBuffer(CVPixelBuffer),   // IOSurface-backed NV12
    Nv12Cpu(Nv12Planes),          // generic path, for tests and future platforms
}
```

- macOS: `VTDecompressionSession`. The work Phase 0 actually has to do:
  Annex B → AVCC (length-prefixed NALUs), build `CMVideoFormatDescription` from SPS/PPS,
  rebuild the session when SPS changes, set `RealTime`, and on `DecodeError` call
  `MediaControl::request_keyframe()` and drop frames until the next IDR.
- The decoder knows nothing about wgpu. The renderer owns GPU interop.
- Windows (Media Foundation) and Linux (VA-API): backlog.

### 2.5 Audio (Phase 3)

Opus from Pion, decoded with `audiopus`, played with `cpal`. Microphone: backlog.

## 3. GUI (`apps/glkvm-gui`)

**eframe + egui on the wgpu backend.** The video is a wgpu texture drawn through an egui
paint callback, not an egui widget.

Video rendering:
- **Zero-copy (default, done)**: the decoder's IOSurface planes are wrapped as Metal textures
  (`CVMetalTextureCache` + `wgpu-hal`) and sampled directly; YUV→RGB in a small WGSL shader,
  drawn letterboxed. Halved process CPU at 2560×1440@60 versus uploading (numbers in
  `docs/protocol.md`).
- **Upload (fallback, `--render upload`)**: lock the `CVPixelBuffer` and copy the NV12 planes
  into two wgpu textures (R8 + RG8) every frame (~330 MB/s at 1440p60). Used automatically
  if the texture cache is unavailable or wrapping a frame fails.

**Power rule**: eframe runs in reactive mode; `request_repaint()` only when a new frame
arrives or the UI changes, never continuous. When the window is minimized or occluded:
stop decoding immediately; after a **30 s grace period**, drop the video stream and keep
`/api/ws` open with `stream=0`. On show, restart and request a keyframe. The grace period
avoids a streamer cold start (seconds) on quick app switches.

Input (v1):
- winit key and mouse events → `glkvm::events`. **Map mode** only: send the physical key
  position (`KeyboardEvent.code`). Correct when layouts match, and the only mode that works
  in BIOS/bootloaders.
- Per-device **target OS profile** (Windows / macOS / Linux / BIOS) drives modifier
  mapping, e.g. Mac client → Windows target maps Cmd→Ctrl.
- **Special keys menu**: Ctrl+Alt+Del, Win/Super, Alt+Tab, PrintScreen, Fn-row.
  This is the v1 answer to OS-reserved shortcuts; a real keyboard grab is backlog.
- Key-handling rules (each one is a bug in Overlook):
  - Drop local auto-repeat events (`repeat == true`). The target generates its own repeat
    from a held key, as with any USB keyboard.
  - Caps Lock: macOS reports it as a state change, not as press/release. Send a full
    press + release on every change, or the target toggles only on every second press.
  - A Cmd/Meta tap on its own is sent (it opens the Start menu or launcher on the target).
    Only app shortcuts we explicitly bind are swallowed.
  - Focus loss → `release_all()` (§2.2).
- Mouse: absolute only in v1. Relative mode depends on U6. Buttons: left, right, middle,
  and back/forward if the device accepts them.
- **Input capture pauses** while any overlay (stats drawer, dialogs, menus) has focus,
  and resumes with its previous state when the overlay closes.
- **Type clipboard** (superseded where possible: with the clipboard agent the text is
  inserted in one step, and typing uses our own CLDR layout tables to avoid the firmware's
  AltGr bug; see `docs/clipboard-bridge.md`, "Inserting text"): `POST /api/hid/print` with the target's keymap, on `Cmd/Ctrl+Shift+V`
  (plain paste is forwarded as keys). Confirm before typing multi-line text (newlines
  become Enter, which runs commands in terminals) or text over 1,000 characters; show
  progress and allow cancel. Details:
  - Send **release-all-modifiers** before typing; the user is still holding Cmd+Shift,
    and the typed text would otherwise arrive as shortcuts.
  - Pass `limit=0`: the firmware's default `limit` is 1024 and **silently truncates**
    (`kvmd/apps/kvmd/api/hid.py`). Chunk long text (~500 chars per request) for progress.
  - Cancel = close the in-flight request: the firmware cancels the typing task and clears
    queued HID events when the client disconnects.
  - Check characters against the target keymap first; untypeable ones (emoji, CJK) are
    reported, not silently dropped.
  - Shortcut conflict: with Cmd→Ctrl mapping, `⌘⇧V` is how a Mac user sends Ctrl+Shift+V,
    which is **paste in Linux terminals**. For the Linux profile, default the binding to
    host-key+V (Right-Cmd+V) instead, and make it configurable everywhere.

Screens (v1):
1. **Device list**: saved devices (the password goes in the OS keychain via `keyring`; the
   auth token is kept in memory only, never in a preferences file), manual add with a
   username field (not hardcoded `admin`), per-device status and thumbnail from
   `/api/streamer/snapshot`. Manual add **verifies** the host: `GET /api/auth/check` and
   `GET /api/init/is_inited` must answer like a GLKVM (status code plus JSON shape), not
   just "a port is open". It suggests `192.168.200.1`, the Comet's USB/AP network default.
2. **Console window**: video, auto-hiding toolbar (special keys, type clipboard, snapshot,
   ATX power/reset, WoL, "Open device settings"), HDMI-signal indicator,
   **other-viewers count** from `/api/system/clients`, fullscreen.
   - In fullscreen, the toolbar appears when the pointer rests in a thin top strip (about
     350 ms), and input capture pauses while it is shown.
   - Window resizing is locked to the video aspect ratio (no letterbox bars in windowed
     mode). "Fit to guest" sizes the window to 1:1 pixels, clamped to the screen.
   - The window title shows `device · WxH · fps · kbps`. A stats drawer shows fps,
     bitrate, decode time, RTT, loss and frame age.

## 4. Shared device state

The device is shared: the web app, other copies of this app and gl-pion may all be
connected. Rules:

| Action | Scope | Rule |
|---|---|---|
| Watching video, sending HID, ATX, WoL | Session / target | Free. ATX power and reset ask for confirmation. |
| `streamer/set_params` (quality, bitrate, FPS, GOP) | **Device-wide, persisted**, affects every viewer | Only from an explicit user action, with a dialog naming the effect. No automatic adaptive quality writes to the device. |
| `gl_webrtc` / saved `video_mode` | **Device-wide, persisted**; kills other clients' Janus or Native sessions | Never changed automatically. If the chosen source can't run under the current mode (U4), the UI explains and offers the switch, showing how many other viewers are connected. |
| Mouse output mode (`is_absolute_mouse`) | Device-wide; may re-enumerate USB on the target (U6) | Explicit user action with confirmation. |
| `hid/set_connected` | Device-wide: `false` cuts HID for every viewer (U8) | Never sent on disconnect. On connect, if `GET hid` reports HID disconnected, show it and offer to connect. |
| `system/set_config` (orientation, `show_cursor`, `keymap`, `video_processing`, mouse/scroll prefs, ...) | **Device-wide, persisted**, and the API replaces the **whole object** | We don't write it in v1. If we ever do: read, change one field, write within one request cycle, and warn that a concurrent change from another viewer can be overwritten. |
| Disconnecting other clients | Device-wide | Explicit, confirmed. |

**Client preferences stay local.** Scroll rate, reverse scrolling, relative sensitivity, the
keymap used for "type clipboard", and the target OS profile are stored per device in our
app. They are seeded from `system/get_config` on first connect and never written back, so
our settings can't change another viewer's experience.

The SDK enforces naming (§2.1); the GUI enforces confirmation.

## 5. Targets and phases

### 5.1 Measurement

**Baseline (B)**: the official GL web app in current stable Chrome, Direct mode, same
1080p60 source, same Mac, same network, measured at the start of Phase 0. Every number
below is recorded for both B and glkvm-gui in `bench/results.md`.

| ID | Metric | Method | Target |
|---|---|---|---|
| M1 | Glass-to-glass latency | Target shows a millisecond clock fullscreen; film target monitor and client window together at 240 fps; 20 samples | p50 ≤ B p50 + 5 ms (no regression); p95 ≤ B p95 |
| M2 | Power while viewing | `sudo powermetrics --samplers cpu_power,gpu_power -i 1000 -n 60`, combined power minus idle, fixed brightness, other apps closed. For B, this automatically includes Chrome's GPU and renderer processes. | ≤ 60 % of B |
| M3 | Power while hidden | Window minimized ≥ 35 s (past the grace period), same method | ≤ 50 mW over idle; network ≤ 10 kB/s |
| M4 | Frame delivery | 5 min of 1080p60 with motion | ≥ 59 fps displayed, < 0.5 % frames dropped |
| M5 | Time to first frame | Click device → first frame | ≤ 1.5 s with streamer running, ≤ 5 s cold |
| M6 | Memory | RSS of one console window after 10 min | ≤ 200 MB |
| M7 | Remote (Phase 3) | Pion over a link shaped to 40 ms RTT, 1 % loss (`dnctl`/`pfctl` dummynet), 5 min | ≥ 30 fps, no freeze > 500 ms |

Note on expectations: the web app's Direct mode in Chromium likely decodes in hardware
through WebCodecs already. The realistic wins are M2/M3 (repaint discipline, not decoding
when hidden), not M1. A result of "same latency, half the power" is a pass.

### 5.2 Phases

| Phase | Deliverable | Done when |
|---|---|---|
| **0. Baseline + spike** (≤ 2 wk) | Measure B (M1–M6). Answer U1–U8. `glkvm-cli view`: login, Direct, VideoToolbox, NV12 shader in a minimal winit + wgpu window, map-mode keyboard + absolute mouse. | `bench/results.md` has B and spike numbers; `docs/protocol.md` answers U1–U8. **Go** if M1 and M2 pass. If M1 passes but M2 doesn't, profile once; if still failing, keep SDK + CLI and stop the GUI. |
| **1. SDK core** | `glkvm` (auth incl. 2FA/two-step, info, capability, clients, streamer state, snapshot, hid incl. print, atx, wol, events), Direct source in `glkvm-media`, CLI | CLI can log in (incl. 2FA), view, snapshot, power-cycle, wake, type text. Fixture tests green, `cargo-deny` green. |
| **2. Console MVP** | GUI per §3: device list + keychain, console window, map mode + OS profiles, special keys, type clipboard, snapshot, ATX, WoL, stats, HDMI state, viewer count, reactive repaint, hidden-window rule, "Open device settings" | M1–M6 pass. Used as the daily console for 2 weeks without falling back to the web app for viewing. |
| **3. Remote** | First: a Pion spike answering "does gl-pion accept plain RTP without FlexFEC?". Then the Pion source (str0m) + Opus playback. Janus only if the spike fails. | M7 passes over the shaped link and over the real VPN. Audio plays. |

After Phase 3, work comes from the backlog, one item at a time, each with its own
done-criterion written when it's pulled.

## 6. Testing

- **Fixtures**: record real JSON responses/events from the Comet for every route the SDK
  implements (scrub tokens and serials), store per firmware version, deserialize in unit
  tests. Every `glkvm` proto type gets a round-trip test. Mutating routes (ATX, upgrade,
  reset) are recorded from captured web-app traffic, not by calling them.
- **Firmware testenv**: the glkvm repo ships a Docker test environment (`testenv/`,
  `make run`) running kvmd against fake hardware. Use it in CI for upstream-kvmd endpoints
  only (auth, info, hid, atx, streamer state, `/api/ws`). GL-specific endpoints call into
  real system scripts and are covered by fixtures and device tests.
- **Real-device tests**: `#[ignore]` integration tests gated on `GLKVM_HOST` and
  `GLKVM_PASSWORD`, covering login, video start (first keyframe decoded), HID round-trip.
  Nothing destructive.
- **Compat watch**: a CI job that diffs the route list in `gl-inet/glkvm`
  `kvmd/apps/kvmd/api/` against Appendix A, plus a manual script that probes the live
  device for the same list (U7), so new or removed endpoints show up.
- **Benchmarks**: M1–M7 are re-run before closing each phase and on every firmware update.
- **Licenses**: `cargo-deny check licenses` in CI.

## 7. Risks

| Risk | Mitigation |
|---|---|
| Undocumented API that GL.iNet changes (e.g. 1.10's adaptive mode killing Janus) | Small API surface (console only), capability detection, per-firmware fixtures, route-diff CI, Direct mode first |
| gl-pion rejects plain RTP without FlexFEC | Phase 3 starts with that spike; fallbacks: Janus (device-wide mode switch, §4), or Direct over VPN with its TCP limits |
| Win is smaller than hoped (WebCodecs is already hardware decode) | Targets are relative to B and focus on power; Phase 0 go/no-go before the GUI is built |
| VideoToolbox details (AVCC, SPS changes, error recovery) eat the spike | U2 answered first; keyframe request on any decode error |
| Changing device settings disrupts other viewers | §4 rules, viewer count in the toolbar |
| Accidentally copying GPL/AGPL code or data | Clean-room rule (§0), own key table from public specs, `cargo-deny` |

## 8. Backlog (pull on demand, no dates)

Each item lists what triggers it. Nothing here is planned until it's pulled.

| Item | Trigger / notes |
|---|---|
| Drag-and-drop file → MSD flash (`msd/write`, streamed with progress) | First time a file needs to reach the target |
| OCR copy (Apple Vision; fallback device Tesseract via `GET /api/streamer/snapshot?ocr=1&ocr_langs=…&ocr_left/top/right/bottom=…`. `/api/streamer/ocr` only reports availability and languages) | Remote → local text need. Also offer "copy region as image" (exact). Shape proven by Overlook: drag a rectangle (or click → ~800 px square around the point), Vision `.accurate` with language correction on that crop, result in a selectable sheet with Copy. Live text-region boxes (`.fast`, every ~650 ms) are optional and off by default. Copy frames out only while OCR mode is on. Mind Vision's bottom-left origin. |
| Network scan | Manual add proves tedious. Sweep each private /24 plus `192.168.200.0/24` on 443/8443/80/8080 with bounded concurrency, and accept a host only via the §3 fingerprint. mDNS is unproven: Overlook browses `_comet._tcp` and `_kvm._tcp`, but whether the firmware advertises them is unverified (and it ignores the SRV port). |
| Tailscale discovery | Remote devices on a tailnet. Read peers from `tailscale status --json` (CLI found via PATH or config), then apply the §3 fingerprint. No guessing from hostname, tags or OS; never install anything. |
| Menu bar item (quick connect, device status) | App used many times a day. No global keyboard monitor (needs Input Monitoring permission and fails silently without it). |
| Quality presets UI | Changing quality from the app instead of the web UI. Device-wide (§4). Use `streamer.features` to gate controls and `streamer.limits` to clamp them, and re-read the streamer state after writing. |
| ~~Audio device picker~~ | Done: Video menu → Play audio → Audio output; the choice and the on/off state are saved per device. |
| Full keyboard grab via [`rdev`](https://github.com/rustdesk-org/rdev) (MIT) `grab`, host-key toggle (Right-Cmd), needs Accessibility permission | Special keys menu proves insufficient |
| Translate keyboard mode | Layout mismatch hurts in practice. Target layouts from Unicode CLDR keyboard data or xkeyboard-config (permissive), never `keymap.csv`. Dead keys and AltGr need care. |
| Relative mouse + pointer lock | Depends on U6; needed for games or some BIOSes |
| Local cursor overlay | Pointer lag noticeable |
| Microphone, camera passthrough | Concrete use case |
| H.265 | Only if U5 says Direct or Pion can carry it |
| mDNS discovery (`mdns-sd`) | More than a couple of devices |
| Multiple devices / screen wall | More than one device in daily use |
| Settings screens (MSD, EDID, HID settings, users/2FA, upgrade, network, VPN, switch/GPIO) | One at a time, only when the web UI becomes the bottleneck for that task |
| Serial console | Device with serial in use |
| Windows (Media Foundation) / Linux (VA-API) | A user on that platform |
| Packaging: `cargo-dist`, macOS signing/notarization | Someone other than the author uses it |
| GL cloud remote connect (`api.glkvm.com`, reverse-engineered) | VPN options prove insufficient |
| Companion agent on the target for real two-way clipboard sync, carried through the Comet. The serial gadget is absent from the RM1 kernel, so the transport is USB Ethernet (RNDIS) + the Comet's existing NAT, with the agent connecting out to the host. Windows/Linux targets only. Findings and design in `docs/clipboard-bridge.md` §0 | RNDIS enable + target test in §0 pass |

## 9. Prior art (read for behavior; respect licenses)

| Project | License | Use |
|---|---|---|
| [gl-inet/glkvm](https://github.com/gl-inet/glkvm) | GPLv3 | API ground truth. Read only. |
| [PiKVM API docs](https://docs.pikvm.org/api/) | docs | Upstream REST and WebSocket semantics. |
| [rcawston/Overlook](https://github.com/rcawston/Overlook) | GPLv3 | Native Swift client: Janus flow, HID encoder, OCR/paste UX. Read only; findings in §10. |
| [gl-inet/glkvm-cloud](https://github.com/gl-inet/glkvm-cloud) | check before use | Self-hosted cloud relay (Go), backlog only. |
| [mantis5x5/kvm-computer-plane](https://github.com/mantis5x5/kvm-computer-plane), [kennypeh85/glkvm-mcp](https://github.com/kennypeh85/glkvm-mcp) | check before use | Small API clients, cross-checks. |
| RustDesk (`src/keyboard.rs`, `translate_keyboard_mode`) | AGPL-3.0 | Keyboard-mode ideas and edge cases. Read only. |
| [rustdesk-org/hwcodec](https://github.com/rustdesk-org/hwcodec) | none | Ideas only; don't vendor. |
| [rustdesk-org/rdev](https://github.com/rustdesk-org/rdev), `arboard` | MIT, Apache-2.0 | Safe dependencies. |

### Why we copy TeamViewer/RustDesk UX but not their features

They run an **agent on the remote machine**, so they can sync clipboards, inject Unicode
and transfer files. A KVM only sees HDMI out and acts as a USB keyboard/mouse; nothing
runs on the target. So: forward Cmd/Ctrl+C/V as keys (copy/paste works *within* the
target), replace local → remote clipboard sync with **type clipboard**, remote → local
with **OCR copy** (backlog), and file transfer with **MSD drag-and-drop** (backlog).

## 10. Findings from Overlook (reviewed 2026-10-28, commit `c3f2f24`)

Overlook is GPLv3, so this section records **behavior and protocol facts only**, written
in our own words (§0). Everything here still gets confirmed against our device in Phase 0
and recorded as fixtures.

### 10.1 Protocol facts it confirms

- **Auth**: `POST /api/auth/login` takes multipart fields `user` and `passwd`. The token is
  in the JSON result (`token` or `auth_token`) or in `Set-Cookie`. `GET /api/auth/check`
  validates a stored token. Error text is in `result.error_msg` or `result.message`.
- **Device fingerprint**: `GET /api/auth/check` (200/401/403 or redirect) plus
  `GET /api/init/is_inited` (`{is_inited, country_code}`) identify a GLKVM.
- **HID** over `/api/ws`: binary frames with a type byte (U1), ping as JSON text.
  `POST /api/hid/print?keymap=<name>` takes the text as a `text/plain; charset=utf-8` body
  and also accepts `limit` and `slow`. Our "type clipboard" uses `slow` for BIOS-class
  targets and does **not** trim the text (Overlook trims, which drops trailing newlines).
  `GET /api/hid/keymaps` → `{keymaps: {default, available}}`.
- **System config** (`GET /api/system/get_config` → `{config: {...}}`), relevant fields:
  `keymap`, `orientation`, `video_mode`, `video_processing`
  (`low_latency_first` / `quality_first`), `show_cursor`, `is_absolute_mouse`,
  `mouse_polling`, `relative_sense`, `scroll_rate`, `reverse_scrolling`
  (`STANDARD|VERTICAL|HORIZONTAL|BOTH`), `mouse_jiggle`, `shortcuts[{keys, label}]`.
  `POST /api/system/set_config` replaces the whole object (§4).
- **Streamer** (`GET /api/streamer`): `features{quality, resolution, h264, zero_delay}`,
  `limits{desired_fps, h264_bitrate, h264_gop}{min, max}`,
  `params{desired_fps, quality, h264_bitrate, h264_gop, zero_delay, resolution}`.
  `set_params` takes query parameters. `video_processing = low_latency_first` should also
  switch our client to zero-buffer presentation; `quality_first` may allow a small buffer.
- **ATX**: `POST /api/atx/power?action=on|off|off_hard|reset_hard`,
  `POST /api/atx/click?button=power|power_long|reset`.
- **MSD** (backlog): `partition_show|connect|disconnect|format` are **GET**s;
  `write?image&prefix&remove_incomplete` takes an octet-stream body;
  `write_remote?url&...` streams NDJSON progress `{image: {name, size, written}}`.
- **EDID** (backlog): `GET /api/upgrade/get_edid` returns a string or `{edid}`/`{EDID}`;
  `POST /api/upgrade/edid` takes multipart field `edid`.
- **Janus** (fallback only): the upgrade needs `Sec-WebSocket-Protocol: janus-protocol`,
  the auth cookie and an `Origin` header. Sequence: `create` → `attach`
  `janus.plugin.ustreamer` → `message {request: watch, params: {orientation, audio,
  video, mic, camera}}` → server offer → `message {request: start}` + answer → trickle
  (`{completed: true}` at the end). Keepalive every 25 s.
- `POST /api/hid/reset` + `POST /api/streamer/reset` is the web app's "reset KVM".

### 10.2 Adopted into this plan

| Lesson | Where |
|---|---|
| Separate audio and video sessions (avoids A/V-sync buffering) | §2.3 |
| Zero playout delay; stall watchdog (5 s first frame, 3 s no frame) | §2.3 |
| Single ordered HID queue; 120 Hz latest-wins abs moves; ±127 relative splitting | §2.2 |
| Wheel accumulation and scaling; `release_all()`; drop auto-repeat; Caps Lock; lone Meta | §2.2, §3 |
| Pause input capture under overlays; aspect-locked window; fit to guest; hover toolbar | §3 |
| GLKVM fingerprint for manual add; `192.168.200.1` suggestion | §3 |
| `hid/set_connected` and whole-object `set_config` are device-wide | §4, U8 |
| Client preferences stay local, seeded from device config | §4 |
| TURN from `/api/turn/get_turn` instead of a hardcoded public STUN | §2.3 |
| OCR flow, network scan, Tailscale discovery, menu bar, quality presets | §8 backlog |

### 10.3 What not to copy

- TLS validation disabled everywhere (we pin, §2.1).
- Auth token stored in plain preferences; hardcoded `admin` user; no 2FA.
- No reconnect for the HID socket; no timeouts on signaling requests; Janus errors
  surfacing as "connection lost".
- One task per HID event (no ordering guarantee).
- `hid/set_connected=false` on disconnect, and unconfirmed device-wide settings writes.
- Global keyboard monitor for app shortcuts (silent permission failure). Note also that
  Overlook binds ⌘⇧V globally; our "type clipboard" uses ⌘⇧V only inside the console.
- Loose discovery: "a port is open on 192.168.200.1" or "Linux host on the tailnet"
  counted as a KVM.
- Dead code that looks real: `HardwareControlManager` (raw TCP on port 8444 with invented
  commands) and `TailscaleManager` are never used. Port 8444 is not a device API.
- EDID preset blobs: GL firmware data, not ours to copy.

## Appendix A: firmware HTTP routes (kvmd 4.82, `/api` prefix)

```
ap:            GET status · POST enable, open_last_mode, close_all_mode
astrowarp:     GET status, show, enable, get_bind_link, get_dynamic_code · POST unbind
atx:           GET / · POST power, click
auth:          POST login, logout, two_step_complete, two_step_approve, two_step_reject, unlock_client
               GET check, two_step_pending, two_step_login (+POST), rate_limit_status, locked_clients; GET /same_check
cloudflare:    GET status · POST start, stop, set_token
custom_screen: GET background, status · POST update_background, delete_background, update_screen_mode, update_time_format, update_date_format
export:        GET prometheus/metrics
fingerbot:     GET battery, local_version, exist, click, push, pull · POST upload, upgrade
gpio:          GET / · POST switch, pulse
hid:           GET /, keymaps · POST set_params, set_jiggler_schedule, set_connected, reset, print,
               events/send_shortcut, send_key, send_mouse_button, send_mouse_move, send_mouse_relative, send_mouse_wheel, send_touch
info:          GET /
init:          GET init, is_inited · POST change_password
log:           GET /
modem:         GET get_status, sim_setting (+POST) · POST enable, input_pin_code, at
msd:           GET /, partition_show, partition_connect, partition_disconnect, partition_format, read
               POST set_params, set_connected, set_enabled, switch_partition, write, write_remote, remove, reset
netbird:       start, stop, login, logout, config, get_info (+ gui_* variants)
recorder:      GET / · POST start, stop
redfish:       GET v1, v1/Systems, v1/Systems/0 · PATCH v1/Systems/0 · POST v1/Systems/0/Actions/ComputerSystem.Reset
repeater:      GET get_saved_ap_list, scan, get_preset_ap_list, get_status · POST connect, disconnect, enable, remove_saved_ap, add_preset_ap, del_preset_ap
rndis:         GET status · POST start, stop
serial:        GET list, check, ws
streamer:      GET /, snapshot, ocr · DELETE snapshot · POST set_params, reset
switch:        GET / · POST set_active, set_active_prev, set_active_next, set_beacon, set_port_params, set_colors, reset,
               edids/create, edids/change, edids/remove, atx/power, atx/click
system:        GET clients, capability, get_network_config, get_param, otg_functions, time, timezone/list, get_config,
               get_firewall_config, get_hostname, ssh_key, ssl_cert, ntp · DELETE clients/{client_id}
               POST set_network_config, set_param, otg_functions, time, timezone, set_config, set_firewall_config,
               set_hostname, ssh_key, ssl_cert, reinit_udc, ntp  (+ gui_get_param / gui_set_param)
tailscale:     status, start, stop, login_url, login_status, logout, config (+ gui_* variants)
turn:          GET get_turn
2fa:           GET create, init, show, is_enabled, delete, verify
upgrade:       POST upload, start, edid, log · GET compare, version, reboot, status, reset_default, download,
               beta/download, download_cancel, download_info, get_edid, edid_list, log
wol:           GET scan, list · POST wake, add, remove
zerotier:      status, start, stop, set_token, leave, auth (+ gui_* variants)
ws:            GET /api/ws (events + HID), /api/hid/ws (gl-pion only), /api/media/ws (Direct H.264)
```
