# Protocol notes (verified on the device)

Device: GL-RM1 "Comet", firmware V1.10.1, kvmd 4.82, Rockchip RV1126, encoder `RV1126-H264`.
Verified 2026-10-28 with `glkvm-cli probe` and the ignored device tests
(`cargo test -p glkvm-media --test device -- --ignored`). Answers PLAN.md §1.1.

## Login

- `POST /api/auth/login` must be **URL-encoded** (`user=…&passwd=…`). A multipart body
  with the same fields is rejected with 403 `Forbidden`.
- Response: `{"ok": true, "result": {"token": "…", "failed_since_last_success": N}}` plus an
  `auth_token` cookie. Failed attempts are counted per client and rate-limited (429).
- Passwords may contain `$`; `.env` values must be read literally (`glkvm::env`).
- **2FA**: no separate endpoint. The 6-digit TOTP code is appended to `passwd`;
  `GET /api/2fa/is_enabled` (no auth) tells whether it is needed.
- **Two-step approval**: login answers `{"two_step_required": true, "two_step_token": …,
  "expires_in": N}`; poll `POST /api/auth/two_step_complete` (form `two_step_token`) until it
  returns a token (`{"status": "pending"}` meanwhile, 403 when rejected or expired).
- An expired token gives HTTP 401; the SDK logs in again once (not for TOTP logins).

## Identity and capabilities

- `GET /api/upgrade/version` → `{"model": "RM1", "version": "V1.10.1 release3"}`. `/api/info`
  has kvmd/ustreamer versions and the SoC but not the GL model or firmware.
- `GET /api/system/capability` → string flags (`cpu_model`, `mipi`, `usb_version`, …).
- `GET /api/system/clients` answers **403** to normal sessions (reserved for the device GUI),
  so a "viewers connected" count is not available to us.
- This RM1 has no ATX board (`atx.enabled = false`); power actions would fail.

## Dangerous GETs

`GET /api/upgrade/reboot`, `upgrade/reset_default`, `2fa/create`, `2fa/delete` and
`msd/partition_*` change state. Never call routes speculatively; the fixture recorder uses a
fixed read-only allowlist.

## Events are diffs

Each subsystem's first event is its full state; later events carry only the top-level keys that
changed (e.g. `{"streamer": {...}}` without `features`/`params`). Merge per top-level key.
`hid` events are always full.

## U1: HID frames on `/api/ws`

Binary message = `[event type][payload]`:

| type | payload |
|---|---|
| 1 key | `[flags][W3C code, ASCII ≤ 32]`; flags bit0 pressed, bit1 finish. With bit7 set the payload is a `u16` BE evdev code instead of a name. |
| 2 mouse button | `[flags][name]`; names `left`, `right`, `middle`, `up` (back), `down` (forward) |
| 3 absolute move | `[x i16 BE][y i16 BE]`, −32768..=32767 |
| 4 relative / 5 wheel | `[squash][dx i8][dy i8]…` (several pairs allowed) |
| 6 touch | `[touching][x i16 BE][y i16 BE]` |

- 115 key names are accepted; notably **no F13–F19** (F20 only) and no `NumpadEqual`.
  `crates/glkvm-cli/src/keymap.rs` only emits accepted names (checked by a test when
  `GLKVM_FIRMWARE_SRC` points at the firmware source).
- Keepalive: JSON `{"event_type":"ping","event":{}}` → `pong` event.
- Mouse + keyboard verified working end to end (absolute mouse, clicks, wheel, typing,
  modifiers, Caps Lock, ISO key swap on a German Mac keyboard).

## U2: Direct video (`/api/media/ws`)

- On connect: text event `media` → `{"video": {"h264": {"profile_level_id": "42E01F"}}}`.
- Client sends `{"event_type":"start","event":{"type":"video","format":"h264"}}`.
- Frames: binary `[0x01][key 0|1][Annex B access unit]`.
  - Keyframes carry SPS + PPS + IDR (NAL 7, 8, 5); delta frames are a single slice (NAL 1).
  - Constrained Baseline: no B-frames, so decode order = display order.
  - Observed: 2560×1440 at 60 fps; ~92 KB keyframe; ~130 B deltas on a static screen
    (0.1–1 Mb/s); GOP 60.
- Binary `[0x00]` = ping, answered with `[0xFF]`. Binary `[0x01]` = keyframe request.
- The server queues up to 32 frames per client; on overflow it clears the queue and forces a
  keyframe.

## U3: what keeps video alive

- `/api/media/ws` alone produces no frames. The streamer runs only while an `/api/ws` session
  **without** `stream=0` is open (`streamer: null` before, encoder online ~1–2 s after).
- With the streamer warm, the first keyframe arrives ~0.8 s after the media socket opens.
- No HDMI input ⇒ `streamer.hdmi.signal = false`, `captured_fps = 0`, and no frames at all
  (no placeholder image). Surface this as "target asleep/off".

## U4 (partial)

- Direct mode works while the saved `video_mode` is `webrtc`. No mode switch needed.

## Pion (`/pion/ws`) and Janus, measured over Tailscale (2026-10-29)

Device in `video_mode = webrtc`, Mac and Comet on a tailnet (direct path, ~25 ms RTT).

- **Signaling**: config `{type, video, speaker, mic, hid, offerer: true}` (+ `flexfecLevel`,
  `orientation`, `camera` when video). SDPs, both directions, are **base64 of the JSON
  `{"type", "sdp"}`**, not plain strings; a plain-SDP offer makes the server reset the
  socket. Candidates are plain `candidate:` strings with `sdpMid`/`sdpMLineIndex`. The server
  sends `server-ice-complete` and its candidates after the answer (host, srflx, relay).
- **Answer**: H.264 PT 108 + RTX 109, NACK, PLI, FIR, transport-cc. No FlexFEC unless offered.
- **Without FlexFEC the stream does not survive loss.** ICE and DTLS connect in ~0.3 s, the
  first keyframe arrives ~1 s in, then ~1 s of 60 fps. The server sends large frames as
  unpaced bursts of 3-12 packets of ~1170 bytes; on this path 3-10 packets of nearly every
  burst are lost (peer ingress loss 20-50 %). We sent NACKs (27 in 10 s) and PLIs, and **no
  retransmission (RTX or otherwise) ever arrived**, and PLI did not produce a keyframe within
  5 s, so str0m's incomplete frames stall the decoder until the watchdog reconnects. Removing
  the transport-cc extension changed nothing. Janus behaved the same (connects, intermittent
  frames, same loss) but the connection survives longer.
- Consequence: Pion/Janus work only on a clean link (LAN) until FlexFEC-03 is decoded or the
  server honors NACK. Over Tailscale use Direct (TCP): 1.0 s to first keyframe, 60 fps.
- **Audio** (own connection, as the web app does it): works on **both** Pion
  (`speaker: true`, video false; base64 SDPs, `client-ice-complete`) and Janus (second handle,
  `watch {audio: true, video: false, mic: false, camera: false}`). Opus 48k/2, ~50 packets/s,
  ~130 kbps, decodes cleanly, ~25 ms RTT. Earlier runs got zero RTP (`bytes_rx = 0`) while the
  target was apparently silent; it started delivering once sound was playing and the web app's
  audio toggle had been turned on. Which of the two mattered is unknown (the toggle is client
  state in the web app, so most likely the target was silent). The web app picks the audio
  transport by `video_mode`: Janus in `webrtc`, Pion in `flex_fec`, none in Direct.
- `opus-decoder` 0.1.1 can panic on some packets (`celt/vq.rs`: `1 << i` on a `u8` with more
  than 8 blocks), only with overflow checks on. The player catches it and recreates the decoder.

## U8 (partial)

- `hid.connected` was `false` while the target was off and `true` once it was on, without us
  calling `hid/set_connected`. HID worked without touching it.

## Things to keep out of logs

- The `streamer`/`turn` events include TURN credentials (`username`, `password`). Never log
  raw event payloads at info level.

## Client-side numbers (Apple M5, release build, 2560×1440@60)

| | |
|---|---|
| VideoToolbox decode (sync) | 1.4–2.4 ms / frame |
| Viewer while hidden | idle: decoding stops, main thread blocked |
| RSS | ~120–145 MB |

Render paths, measured with `glkvm-cli view --render <path> --bench 20` (process CPU via
`getrusage` over 20 s after a 3 s warm-up, window visible, 60 fps, mostly static screen;
two runs each, plus one extra zero-copy run at 4.6 %):

| | upload (fallback) | zero-copy (default) |
|---|---|---|
| Process CPU | 16.0–17.3 % of one core | 4.6–8.3 % |
| CPU per frame | 2.7–2.9 ms | 0.8–1.4 ms |
| Main-thread work per frame | 1.55–1.65 ms | 0.25–0.45 ms |
| Frame arrival → presented | 3.0–3.2 ms | 1.4–2.0 ms |
| Pixel copies per second | ~330 MB (2560×1440 NV12) | none |

`VTDecoderXPCService` used no measurable CPU in either mode (hardware decode).
