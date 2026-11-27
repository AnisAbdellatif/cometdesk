# tests
- `device.rs`: Tests against a real device; ignored by default, run with credentials in `.env`.
- `data/`: Small H.264 sample streams (keyframe and delta) used by the `vt.rs` decoder tests.
- `mock_direct.rs`: Direct video tests against `cometdesk-mock` (no hardware; includes a VideoToolbox decode check)
