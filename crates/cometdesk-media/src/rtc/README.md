# rtc

- `mod.rs`: Shared str0m WebRTC peer setup (`Peer`, `configure_rtc`), candidate and keyframe helpers, constants shared with audio.
- `video.rs`: Video reconnect loop (`supervise`) and packet pump: FlexFEC repair, frame reassembly, keyframe requests.
- `audio.rs`: Audio-only WebRTC supervision and Opus packet pump (`supervise_audio`).
- `tests.rs`: Unit tests for keyframe detection, SDP candidate parsing, and peer config.
