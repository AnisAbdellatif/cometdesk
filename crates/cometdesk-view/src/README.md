# cometdesk-view/src

- `lib.rs`: Crate root re-exporting video pipeline, input mapper, keymap, and QoS helpers.
- `input.rs`: Winit keyboard/pointer/scroll event translator to `HidCommand`.
- `keymap.rs`: Winit `KeyCode` to W3C `KeyboardEvent.code` mapping table.
- `pipeline.rs`: Decode-and-render worker thread connecting `VideoSource` to `VtDecoder`.
- `qos.rs`: macOS `QOS_CLASS_USER_INTERACTIVE` thread priority helper.
- `video.rs`: wgpu NV12-to-RGB shader renderer (`VideoRenderer`, `Frame` upload).
- `zerocopy.rs`: macOS `IOSurface` / Metal zero-copy NV12 texture import for wgpu.
