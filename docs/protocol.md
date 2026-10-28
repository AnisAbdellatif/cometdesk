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
