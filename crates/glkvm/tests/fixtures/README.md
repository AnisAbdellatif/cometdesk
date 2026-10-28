# Device fixtures

API responses recorded from real devices, one directory per model and firmware. Unit tests in
`src/api/*` deserialize them, so a firmware change that breaks a type shows up as a failing
test.

Record a new set (read-only routes only, secrets and identifiers scrubbed):

```bash
cargo run -p glkvm-cli -- record-fixtures crates/glkvm/tests/fixtures/<model>-<firmware>
```

The target must be awake: `streamer_running.json` is recorded while the streamer captures.
Review the files before committing; the scrubber replaces serials, UUIDs, MACs, tokens and
credentials and renames virtual-media images, but new firmware may add new fields.

| Directory | Device | Firmware |
|---|---|---|
| `rm1-v1.10.1` | GL-RM1 "Comet" (RV1126, no ATX board) | V1.10.1 release3, kvmd 4.82 |
