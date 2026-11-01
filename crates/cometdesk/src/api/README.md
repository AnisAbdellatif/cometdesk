# cometdesk/src/api

- `mod.rs`: Module exports for all device REST API groups.
- `atx.rs`: ATX power state and power/reset button actions (`/api/atx`).
- `bridge.rs`: KVM-side RNDIS clipboard bridge installation, status, and removal.
- `capabilities.rs`: Feature discovery probe across firmware endpoints.
- `edid.rs`: Display EDID preset listing, reading, and switching.
- `hid.rs`: REST HID endpoints (keymap list/switch, print, reset).
- `info.rs`: Device hardware, firmware, and system info (`/api/info`).
- `msd.rs`: Virtual media / Mass Storage Device image upload, mount, and removal (`/api/msd`).
- `streamer.rs`: Video streamer status, quality, codec, resolution, and snapshot APIs (`/api/streamer`).
- `system.rs`: Video mode, encoder mode, latency mode, and reboot controls.
- `wol.rs`: Wake-on-LAN target list and magic-packet trigger (`/api/wol`).
