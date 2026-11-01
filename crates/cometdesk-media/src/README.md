# cometdesk-media/src

- `lib.rs`: Crate root (`Codec`, `EncodedFrame`, transport exports).
- `annexb.rs`: H.264/H.265 Annex-B NAL unit scanner and IDR keyframe classifier.
- `audio.rs`: Opus WebRTC audio receiver and CoreAudio output stream.
- `direct.rs`: Direct WebSocket video transport (`/api/media/ws`).
- `flexfec/`: FlexFEC-03 forward error correction decoder for Pion video.
- `janus.rs`: Janus WebRTC signaling client (`/janus/ws`).
- `pion.rs`: Pion WebRTC signaling client (`/pion/ws`).
- `rtc/`: Shared str0m WebRTC UDP engine for video and audio.
- `source.rs`: Unified `VideoSource` handle, `SourceState`, `MediaStats`, and frame rate tracker.
- `vt.rs`: Hardware H.264/H.265 VideoToolbox decoder (`VtDecoder`, NV12 `CVPixelBuffer` output).
