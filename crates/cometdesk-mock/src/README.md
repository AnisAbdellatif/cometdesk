# cometdesk-mock

Dev-only fake GLKVM device (TLS + REST + `/api/ws`) for SDK and CLI tests, built from recorded fixtures.

- `lib.rs`: `MockDevice`, `Config`, `Call`, shared state, TLS accept loop, connection dispatch.
- `http.rs`: HTTP/1.1 request parsing, `Reply`, JSON-envelope writer.
- `routes.rs`: REST routes (login, 2FA, authed endpoints).
- `ws.rs`: `/api/ws` upgrade, initial state events, ping/pong, HID frame capture, pushed events.
- `fixtures.rs`: embedded rm1-v1.10.1 recordings, `result(name)`.
