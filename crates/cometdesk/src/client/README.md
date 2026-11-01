# client

- `mod.rs`: Pre-login `Connection`, `Credentials`, `PendingLogin`, `SystemConfig`, and envelope parsing.
- `session.rs`: Authenticated `Session` REST, multipart, raw, and WebSocket methods with 401 re-login.
- `tests.rs`: Unit tests for host normalization, credentials, and API envelope parsing.
