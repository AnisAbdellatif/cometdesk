# str0m 0.24.1, patched for cometdesk

Upstream: https://github.com/algesten/str0m (MIT / Apache-2.0), version 0.24.1 from crates.io,
minus `tests/`, `examples/`, `docs/` and `logo/`.

str0m has no FlexFEC, and it drops RTP of an unnegotiated payload type before any hook sees
it. The patch adds just enough for an application to decode FEC itself:

- `Rtc::set_fec_payload_type(Some(pt))`: packets of that payload type are decrypted (the
  repair stream has no `Media`, so its SRTP rollover is tracked in `Session::handle_fec_rtp`)
  and every received media packet is tapped. Both come out of `poll_output()` as
  `Event::FecRtp(Box<FecRtp>)`; `FecRtp::tail` holds the bytes between the fixed header and
  the payload (CSRCs, header extensions), which FEC protects.
- `Rtc::inject_rtp(now, header, payload)`: delivers a packet rebuilt from FEC through the
  normal receive path (sequence extension, receive register so it is not NACKed, depayloader).

All changes are marked `cometdesk patch` in `src/lib.rs` and `src/session.rs`. Drop this
directory and the `[patch.crates-io]` entry in the workspace `Cargo.toml` once str0m
upstreams an equivalent.
