# cometdesk

Native console for [GL.iNet GLKVM](https://github.com/gl-inet/glkvm) devices (GL-RM1 "Comet"
and friends): low-latency hardware-decoded video, keyboard/mouse, power control, clipboard and
file transfer. macOS (Apple Silicon) first.

## Layout

| Path | What |
|---|---|
| `crates/cometdesk` | Rust SDK for the device REST/WebSocket API, HID, login, TLS pinning |
| `crates/cometdesk-media` | Video/audio transports (direct, Pion, Janus) and decode |
| `crates/cometdesk-view` | Render pipeline, input mapping, QoS |
| `crates/cometdesk-clip` | Noise-secured clipboard / file / notification protocol |
| `crates/cometdesk-cli` | Command-line tool |
| `apps/cometdesk-gui` | The desktop console |
| `apps/cometdesk-agent` | Target-side agent (clipboard, info, update) |
| `bridge/` | KVM-side bridge (Python) |

## Build

```sh
cargo build --release --workspace
cargo run -p cometdesk-gui
```

Verify:

```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

## Docs

- [PLAN.md](PLAN.md): goals and decisions
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): data flow
- [docs/protocol.md](docs/protocol.md): device protocol

## License

[MIT](LICENSE). Clean-room: GPL/AGPL sources were read for behavior only.
