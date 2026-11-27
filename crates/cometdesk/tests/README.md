# tests
- `login.rs`: Login, re-login and certificate pinning against `cometdesk-mock` over real TLS.
- `api.rs`: REST wrappers (info, ATX, MSD, streamer, WoL, print) against `cometdesk-mock`.
- `events.rs`: `EventSession` over `/api/ws`: state merge, HID order, kickout, reconnect, re-login.
- `fixtures/`: API responses recorded from real devices, one directory per model and firmware (see its README).
