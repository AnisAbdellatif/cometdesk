# cometdesk-mock

Dev-only fake GLKVM device (TLS + REST + `/api/ws`) for SDK and CLI tests, built from recorded fixtures.

- `lib.rs`: `MockDevice`, `Config`, `Call`, shared state, TLS accept loop, connection dispatch.
- `http.rs`: HTTP/1.1 request parsing, `Reply`, JSON-envelope writer.
- `routes.rs`: REST routes (login, 2FA, authed endpoints).
- `ws.rs`: `/api/ws` upgrade, initial state events, ping/pong, HID frame capture, pushed events.
- `fixtures.rs`: embedded rm1-v1.10.1 recordings, `result(name)`.
- `main.rs`: standalone binary (`cargo run -p cometdesk-mock -- --atx`), runs the mock until Ctrl-C
- `device.rs`: mutable device state (streamer params, config, MSD, WoL, EDID)
- `stateful.rs`: routes that depend on earlier writes, plus snapshot and EDID; `snapshot.jpg` is its canned picture
- `tests.rs`: unit tests for wire helpers
- `faults.rs`: fault injection (delay, forced status, dropped requests)
- `tests/wire.rs`: raw-TLS protocol tests (keep-alive, pipelining, HEAD, Expect, limits, timeouts, WS faults)
- `media.rs`: `/api/media/ws` direct H.264 from `clip.h264` (640x360, 30 fps, GOP 30, looped)
