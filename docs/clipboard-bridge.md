# Clipboard bridge through the Comet (design + feasibility)

Status: investigated 2026-10-28 from firmware source (gl-inet/glkvm, kvmd 4.82), then
checked over SSH on the reference Comet (GL-RM1, firmware V1.10.1). **Read §0 first: the
serial path in §1–§2 is not possible on this hardware; the transport is now USB
Ethernet (RNDIS) + NAT.** §3–§6 (agent, protocol, security) still apply, except that
the transport is TCP instead of a serial line.

## 0. On-device findings (2026-10-28)

Read-only checks over SSH (`root@192.168.8.174`, Buildroot, sysvinit, `/etc` on a
persistent overlay at `/userdata/overlay`):

| Transport | Kernel | State on device | Verdict |
|---|---|---|---|
| USB serial gadget (ACM) | **Absent**: no `acm_bind`, `gser_bind`, `gs_start_io` in `/proc/kallsyms`; no modules (`/lib/modules` missing) | `/dev/ttyGS*` absent, `otg.devices.serial.enabled: false` | **Impossible** without a new kernel from GL.iNet |
| USB Ethernet, ECM (kvmd's default `driver: ecm`) | Absent (`ecm_bind` missing) | — | Impossible |
| USB Ethernet, **RNDIS** | **Present** (`rndis_bind`) | `otg.devices.ethernet.enabled: false`, no `usb0` interface | **Possible**: needs a config override + `kvmd-otg` restart |
| FunctionFS (userspace USB function) | Present (`ffs_alloc`, used for MTP) | `ffs.mtp` | Possible, but needs our own daemon on the Comet |
| Spare HID function `hid.usb3` | Present | Touch digitizer (usage page 0x0D), unlinked | Not a data channel |

`/usr/sbin/kvmd-rndis start` (called by `POST /api/rndis/start`; not exposed in the GL
web UI) does **not** create the USB function; it only configures an existing `usb0`:
`192.168.5.1/24`, `udhcpd` handing out `192.168.5.2–32` with **router 192.168.5.1 and DNS
8.8.8.8**, `ip_forward=1`, and NAT (`MASQUERADE` out `eth0`). Forwarding is
`usb0 → eth0` plus established replies only: the LAN cannot open connections to the
target.

### New path

```
target: cometdesk-agent ──TCP──▶ 192.168.5.1 (Comet usb0) ──NAT out eth0──▶ host:47474 (cometdesk-gui listens)
```

- No Comet code changes: one config override, then the existing `kvmd-rndis`.
- The **agent connects out** to the host; the host listens on one port, firewalled to
  the Comet's LAN IP. The host address and port come from pairing (§6).
- TCP gives reliability and ordering, so §4's COBS/CRC framing reduces to
  length-prefixed frames inside the Noise session.

Costs and caveats:
- Enabling it: write `/etc/kvmd/override.d/90-cometdesk-rs-rndis.yaml` with
  `otg: {devices: {ethernet: {enabled: true, driver: rndis, start: true}}}` (and fixed
  `host_mac`/`kvm_mac` so the target keeps one interface identity), restart
  `/etc/init.d/S50kvmd-otg`, then `POST /api/rndis/start`. The USB device re-enumerates,
  so keyboard and mouse drop for a moment. Not toggleable from the API; check whether it
  survives reboot and firmware upgrade.
- **Target routing side effect**: the DHCP lease advertises the Comet as router and
  8.8.8.8 as DNS. On Linux (NetworkManager gives wired links metric 100, below Wi-Fi's
  600) and often on Windows, the target's **internet traffic would move to the USB link
  via the Comet**. Fix on the target: mark the connection never-default and ignore its DNS
  (`nmcli con modify <conn> ipv4.never-default yes ipv4.ignore-auto-dns yes`; on Windows,
  a high interface metric and no gateway). The agent installer should do this.
- **No macOS targets**: macOS has no RNDIS driver, and the kernel has no ECM/NCM.
  Windows (built-in RNDIS) and Linux (`rndis_host`) work.
- NAT goes out `eth0` only: the host must be reachable from the Comet's Ethernet LAN.
  A host on Tailscale only, or behind another router, isn't reachable. For that case, a
  relay on the Comet (`socat` is installed) would let the host connect *in*, at the cost
  of on-device software.

### Applied on the reference Comet (2026-10-28)

- Wrote `/etc/kvmd/override.d/90-cometdesk-rs-rndis.yaml` (ethernet `enabled: true`,
  `driver: rndis`, `host_mac 02:47:4b:56:4d:01`, `kvm_mac 02:47:4b:56:4d:02`), restarted
  `/etc/init.d/S50kvmd-otg`, ran `kvmd-rndis start`.
- Result: the gadget config links `hid.usb0-2` and `rndis.usb0`, the UDC is bound,
  `usb0` is `192.168.5.1/24`, `udhcpd` is up, and kvmd (incl. HID workers), Janus, Pion
  and nginx are running.
- **End-to-end test passed** (Linux laptop target, NetworkManager):
  - After waking, the target enumerated the Comet at high speed with HID + RNDIS and got
    DHCP lease `192.168.5.2`. Comet → target ping: 0.8 ms.
  - Confirmed side effect: before the fix the target routed its internet traffic via the
    Comet (19 NAT flows, growing). After
    `nmcli con modify <conn> ipv4.never-default yes ipv4.ignore-auto-dns yes`, 0 packets
    were forwarded in 10 s.
  - Agent path: on the target, `ip route add <host>/32 via 192.168.5.1 dev <usb-if>`,
    then a TCP connect to `<host>:47474`. Received on the Mac. Comet conntrack:
    `src=192.168.5.2 dst=192.168.8.185 dport=47474`, reply `dst=192.168.8.174`
    (masqueraded).
  - Lesson: **don't bind the socket to the USB interface** (`SO_BINDTODEVICE` /
    `curl --interface`). With never-default the interface has no route to the host. The
    agent installs a `/32` host route via `192.168.5.1` instead (persistently with
    `nmcli con modify <conn> +ipv4.routes "<host>/32 192.168.5.1"`). The `/32` beats the
    LAN's connected route, so only host-bound traffic goes through the Comet.
  - Side note: after `usb0` appeared, the Comet's WebRTC servers also try `192.168.5.1` as
    an ICE host candidate (stale ARP entries for STUN IPs on `usb0`). Harmless; the
    Comet's own default route stays on `eth0`.
- `kvmd-rndis start` is **not persistent**: after a Comet reboot the gadget function
  comes back (override file), but DHCP/NAT need `POST /api/rndis/start` (our app should
  call it on connect).
- Undo: `rm /etc/kvmd/override.d/90-cometdesk-rs-rndis.yaml; kvmd-rndis stop;
  /etc/init.d/S50kvmd-otg restart`.

### Removing the limits: a relay on the Comet (`cometdesk-bridge`)

The NAT path above has three limits:
- the host must be on the Comet's LAN;
- macOS targets can't use RNDIS;
- the target needs routing changes.

All three go away if **both sides connect *to* the Comet** and a small relay on the
Comet pairs them:

```
target: cometdesk-agent ──USB link──▶ cometdesk-bridge (on Comet) ◀──wss /cometdesk-bridge/ws── cometdesk-gui
                                                               (any path that reaches the
                                                                Comet's web UI)
```

- **True remote for free.** The GUI side is just another WebSocket path on the
  Comet's nginx. Every way of reaching the web UI already tunnels nginx: LAN,
  Tailscale/ZeroTier/NetBird on the Comet, Cloudflare tunnel, GL cloud, self-hosted
  cometdesk-cloud. So the clipboard works wherever video and HID work. The host never
  needs to be reachable from the Comet, and nothing is NATed.
- **Auth for free.** It ships as a kvmd *extra*, GL's own mechanism for `/pion/ws`,
  `/lws/ws` and `/api/media/ws`: `/usr/share/kvmd/extras/cometdesk-bridge/` with
  `manifest.yaml`, `nginx.ctx-http.conf` (upstream to a unix socket) and
  `nginx.ctx-server.conf` (`location /cometdesk-bridge/ws`, with `loc-proxy.conf` and
  `loc-websocket.conf`). nginx's server-level `auth_request /auth_check` applies to it
  automatically: the same cookie or `?auth_token=` as the rest of the API.
- **End-to-end encryption stays.** Agent and GUI run Noise over the relay (§6), so the
  Comet and any cloud proxy only see ciphertext. The relay is a dumb pipe: it pairs one
  agent connection with one GUI connection, forwards frames, and buffers nothing beyond
  a small window.

**USB link options on the agent side:**

| Link | Targets | How | Notes |
|---|---|---|---|
| **RNDIS, no NAT** (v1) | Windows, Linux | Agent connects to `192.168.5.1:47474`; the relay listens on `usb0` only | Replace `kvmd-rndis` with our own setup: `usb0` up, `udhcpd` config **without** `router`/`dns` options, `ip_forward` off. The target then gets a link-local-only network: no default-route takeover, no `nmcli` fix, no host route, and the target can't reach the LAN through the Comet. |
| **FunctionFS vendor bulk interface** (v2) | **macOS**, Windows, Linux | The relay owns the existing `ffs.mtp` FunctionFS slot (MTP is `enabled: true, start: false`, and nothing serves it), writes a vendor-class interface with 2 bulk endpoints plus **Microsoft OS 2.0 descriptors** so Windows auto-binds WinUSB. Agent uses the `nusb` crate (pure Rust, IOKit/WinUSB/usbfs) | No network at all, no driver installs. Linux needs a udev rule for non-root access. Can't be combined with MTP. FunctionFS descriptors must be written before the gadget binds the UDC, so it needs ordering with `kvmd-otg` (verify). |
| Userspace CDC-ECM/NCM over FunctionFS + TUN | macOS too | `/dev/net/tun` exists (`tun_chr_open` in kallsyms) | Gives macOS a real network link, but more work than the bulk interface. Only if a network link is needed for something else. |

**Relay implementation**:
- A static Rust binary (`armv7-unknown-linux-musleabihf`), so it doesn't depend on the
  firmware's Python. A Python/aiohttp prototype runs as-is: Python 3.12 + aiohttp 3.10
  are on the device.
- Started by `/etc/init.d/S99cometdesk-bridge`.
- Exposes `GET /cometdesk-bridge/ws` (GUI side) and the USB-side listener, plus
  `GET /cometdesk-bridge/status` (version, agent connected, link type) so the GUI can detect
  whether the bridge is installed and current.

**Install / update / remove from the GUI** ("Enable clipboard bridge"):
- Over SSH, with the user's key or password, entered once and not stored unless asked.
- Copies the binary, extra, init script and OTG override, restarts `S50kvmd-otg`
  (warn: USB re-enumerates) and `S99kvmd-nginx`. Idempotent and versioned.
- Firmware upgrades may wipe the overlay (unverified): the GUI sees a 404 on
  `/cometdesk-bridge/status` and offers to reinstall.

**Remote paths to verify**:
- [ ] LAN `https://<comet>/cometdesk-bridge/ws`
- [ ] Tailscale on the Comet
- [ ] Cloudflare tunnel
- [ ] GL cloud (glkvm.com device URL): does its proxy pass arbitrary paths and
  WebSockets? `/janus/ws`/`/pion/ws` work there, so likely.
- [ ] self-hosted cometdesk-cloud (open source, check its proxy rules)

### Prototype results (2026-10-28, `bridge/`)

Installed with `bridge/install.sh` (removed by `bridge/uninstall.sh`):
- `cometdesk_bridge.py` (Python/aiohttp, run by `/etc/init.d/S99cometdesk-bridge`);
- the kvmd extra `cometdesk-bridge`;
- `udhcpd` with no router or DNS;
- an `iptables` chain allowing only DHCP, ICMP and TCP 47474 in from `usb0`, plus
  `FORWARD -i usb0 -j DROP`; GL's NAT removed.

- nginx without a login: `401` on `/cometdesk-bridge/status` and on the WebSocket handshake.
  With the session cookie: `101`.
- On-device loopback (unix socket ↔ `192.168.5.1:47474`): both directions forwarded;
  status shows both sides connected.
- **End to end**: laptop agent stand-in (`bash /dev/tcp/192.168.5.1/47474`) ↔ relay ↔
  nginx ↔ `bridge/test_client.py` on the Mac. `hello-from-target` received on the Mac;
  `hello-from-mac` delivered to the agent (relay counters and 0 pending). The order of
  connecting doesn't matter: the relay buffered the agent's message until the client
  connected.
- No routing changes needed on the target any more (only the old `/32` test route had to
  be deleted).
- Found on the way: `kvmd-rndis stop` leaves GL's `udhcpd` running, still advertising
  the Comet as router, and that `udhcpd` ignores SIGTERM. The init script kills any
  foreign `udhcpd` (TERM then KILL) and runs its own in the foreground under its own
  pidfile, because `start-stop-daemon --exec` can't tell BusyBox applets apart.

### Implemented: agent, protocol and GUI (2026-10-28)

What exists:

| Piece | Where | Notes |
|---|---|---|
| Protocol | `crates/cometdesk-clip` | Sans-IO. Framing: `u32` BE length + one Noise message. Noise `NNpsk0_25519_ChaChaPoly_BLAKE2s`, prologue `cometdesk-clip v1`, PSK = 32-byte pairing secret; GUI is the initiator. Messages (postcard): `Hello{protocol, os, name, version}`, `Policy{agent_may_send}` (GUI → agent), `Start{id, kind, len, sha256}` + `Chunk`s (60 KiB) for text or PNG up to 16 MiB, `Ping`/`Pong`. `Dedup` suppresses echoes by a content key (text with normalized line endings; image pixels, not PNG bytes). Pairing code: 52 lowercase base32 chars. 12 unit tests. |
| Relay v0.2 | `bridge/cometdesk_bridge.py` | Session rule: when either side connects or drops, a counterpart paired with the old connection is closed and stale bytes are dropped; a side still waiting for its first partner keeps its buffer (the GUI's handshake). `bridge/selftest.py` checks 4 scenarios on the device. |
| Agent | `apps/cometdesk-agent` | `pair <code>` (secret in `~/.config/cometdesk-agent/pairing`, mode 0600; `%APPDATA%` on Windows), `run [--relay HOST:PORT]`, `unpair`. Blocking std, one clipboard thread owning `arboard` (X11/Wayland selection owners must stay alive). Polls: text every 0.5 s, images every 2 s when there is no text; on Windows only when `GetClipboardSequenceNumber` changes. Sends only what changes while connected, and only if the GUI's policy allows. Reconnects with backoff; drops a session silent for 45 s. |
| SDK | `crates/cometdesk/src/api/bridge.rs` | `Session::bridge_status()` (`None` = not installed), `BRIDGE_WS`. |
| GUI | `apps/cometdesk-gui/src/clipsync.rs`, toolbar in `console.rs` | "Clipboard" menu: status, *Pair agent…* (dialog shows `cometdesk-agent pair <code>` and can type it on the target), direction (both / only to target / only from target, saved per device), Pause, *Pair again*, *Forget pairing*. Secret in the Keychain (`clipboard-bridge:<user>@<host>`). Watches `NSPasteboard.changeCount`; never sends content flagged `org.nspasteboard.ConcealedType`/`TransientType`. Uses `/cometdesk-bridge/status` to tell "no agent" from "agent not answering" (resets after 15 s). "Type clipboard" stays as the no-agent fallback. |

Building the Linux agent on a Mac with no C toolchain:
`CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld CARGO_PROFILE_RELEASE_STRIP=symbols cargo build --release -p cometdesk-agent --target x86_64-unknown-linux-musl`
gives a static 1.8 MB binary. This needs snow without its `std` feature: in snow 0.10.0, `std`
enables `ring/std` unconditionally, which pulls in ring's C code.

**End-to-end test** (Mac GUI ↔ Comet relay ↔ agent on the Linux laptop `asus-tuf`, LAN):
- pairing via the dialog's "Type it on the target";
- handshake;
- Mac → laptop text (45 chars), pasted on the laptop;
- laptop → Mac text (24 chars), arrived on the Mac clipboard;
- Mac → laptop image (320×200 PNG, ~68 KB) in the same session;
- no echo in either direction (including the image read back by the agent).

Not verified yet:
- the Windows agent: it compiles for `x86_64-pc-windows-msvc` but hasn't been linked or run;
- which session type the laptop ran (X11 or Wayland); GNOME on Wayland lacks data-control and
  falls back to XWayland;
- concealed-content detection on the agent side (Linux `x-kde-passwordManagerHint`,
  Windows `ExcludeClipboardContentFromMonitorProcessing`): only the Mac side checks today;
- remote paths (checklist above);
- surviving a Comet reboot;
- agent autostart and distribution (virtual-drive delivery);
- a Rust relay.

**Upstream asks** (would remove the SSH install step): ship the kernel with
`CONFIG_USB_CONFIGFS_ACM`/`NCM`, add serial/ethernet to `_OTG_FUNC_MAP`, and allow
user extras that survive upgrades.

### Inserting text instead of typing it (protocol v2, 2026-10-28)

Typing a clipboard key by key goes wherever the focus is: if the user switches windows
mid-way, the rest lands in the wrong place. With an agent connected, "Insert clipboard"
hands the whole text over in one step (`Msg::Insert{id, text, paste}`, up to `MAX_INSERT`
= 60 KiB):

- **Windows**: one `SendInput` call with `KEYEVENTF_UNICODE` events (Enter/Tab as keys).
  The input queue takes it at once; layout and AltGr don't matter; the clipboard is untouched.
- **Linux / macOS** (and Windows if `SendInput` is refused, e.g. elevated window): the agent
  puts the text on the clipboard and replies `PasteNow(key)`; the GUI sends that single
  keystroke over the KVM (Ctrl+V, Ctrl+Shift+V for terminals, Cmd+V on macOS) and answers
  `InsertPasted`; 1 s later the agent restores the previous clipboard if it still holds
  the inserted text. A real keyboard may paste on Wayland, where programs can't fake input.
  Terminal detection reads the active window's `WM_CLASS` (X11/XWayland); per device the
  user can force Ctrl+V or Ctrl+Shift+V instead of Auto.

Without an agent (or for longer text) the GUI still types, but with its own engine:

**Why AltGr characters got lost**: the firmware (`kvmd/keyboard/printer.py`) sends AltGr
as Left Ctrl + Right Alt. Windows reads that as AltGr; Linux and macOS read a Ctrl
shortcut, so `| @ \ [ ] { } ~ €` on German and similar layouts never arrive.
`cometdesk::layout` types from Unicode CLDR release-43 keyboard tables (`scripts/gen-layouts.py`
generates `layout/data.rs`; Unicode License v3): AltGr is Right Alt alone (Option on macOS
targets), dead-key characters get a following Space, composed ones (`â`) both strokes.
Windows tables serve Linux too; BIOS always gets US. Characters the layout can't type are
skipped and reported. Keymaps we have no table for fall back to `hid/print`.

### Telemetry, session actions, files, notifications (protocol v7, 2026-10-29)
All carried by the same Noise session; `PROTOCOL` was 7 here (now 8, below).

- **Telemetry**: `Info.load` (CPU %, memory, system drive). Not part of `same_as`; the agent
  resends `Info` every 10 s. macOS targets report disk only. The "Target" menu title shows
  `CPU · RAM` and turns amber when the session is locked.
- **Session actions**: `Power{id, action}` (lock, sleep, log off, restart, shut down) answered
  by `Ack{id, result}`. Run through `loginctl`/`systemctl`, `rundll32`/`shutdown.exe`,
  `pmset`/`osascript`. `Ack` also answers `SetLayout{id, layout}` (GNOME `gsettings` current
  source, else `setxkbmap` reorder; Windows `WM_INPUTLANGCHANGEREQUEST`; not on macOS).
- **Files**: `FileOffer{id, name, size, purpose}` → `FileData`* → `FileEnd{sha256}`, answered
  by `FileResult{id, Ok(path) | Err}`; `FileCancel` from either side. Both directions.
  The receiver (`cometdesk_clip::FileReceiver`) sanitizes the name, writes a `.part` file and
  renames after the hash matches. `Purpose::Save` goes to Downloads (GUI → agent only);
  `Purpose::Clipboard` goes to a staging folder (`<tmp>/cometdesk-clip/<nonce>-<id>/`, removed
  after 24 h), and `FilesReady{ids}` makes the receiver put the batch on its clipboard
  (`Content::Files`). Copied files are sent automatically only up to 64 files / 64 MiB in
  total and never folders; the rest says so and points to "Send file". Dropping files on the
  window or "Send file" streams them to Downloads when the agent is connected (otherwise
  the drop still goes to the MSD upload).
- **Region copy**: "Copy region as image" crops the agent's exact screenshot of the main
  monitor to the rectangle dragged on the video. Assumes the KVM shows the main monitor.
- **Notifications**: `Notify{title, body}` agent → GUI. `cometdesk-agent notify <title> [body]`
  hands a message to the running agent over the loopback instance port (rate limited:
  burst of 5, one more per 2 s); the agent also notifies at 15 % battery when unplugged. The
  GUI shows a toast while focused and a macOS notification (via `osascript`, texts passed as
  arguments) otherwise.

### Volume, displays, file browser, agent updates (protocol v8, 2026-10-29)
`PROTOCOL` is 8; `Hello` gained `arch`, so an older agent's hello no longer decodes and shows
as "outdated" (`Error::Version(0)`). Reinstall the agent once; later updates are automatic.

- **Folders in clipboard files**: copied folders go as their files, named `folder/sub/file`
  (`cometdesk_clip::expand`, links not followed, depth 16). The receiver sanitizes every
  component, stages the files of one folder together and puts the folder itself on the
  clipboard. Limits: 512 files, 64 MiB in total. Empty folders are lost.
- **Volume**: `Info.volume` (percent, muted) and `SetVolume{id, level, mute}` → `Ack`.
  Linux `pactl`, else `wpctl`, else `amixer`; Windows `IAudioEndpointVolume` (default render
  endpoint); macOS `osascript`. Target menu: slider (sent on release) and Mute.
- **Displays**: `Display.modes` (resolutions the display accepts) and `SetDisplay{id, display,
  mode, primary}` → `Ack`. Linux `xrandr --output … --mode/--primary` (X11 and XWayland);
  Windows `ChangeDisplaySettingsExW` (a new primary moves the others by its position); not
  on macOS. After changing, "Match the EDID" offers the Comet's EDID preset whose label names
  the new resolution (`1920x1080`, `1080p`), so the target keeps offering that mode.
- **File browser / download**: `ListDir{id, target}` → `DirList{id, Ok(Listing) | Err}`
  (`DirTarget`: home, roots, a path, a child, the parent; cut to one message, `truncated` says
  so) and `Fetch{id, dir, name}` → `Ack`, after which the agent streams the file as a
  `FileOffer` with `Purpose::Save`, which the GUI saves to Downloads (it accepts such an offer
  only for a fetch it asked for). The "Files" button opens the window. The agent shows what
  the user it runs as can read; the paired GUI is that user's own machine.
- **Agent update**: `Purpose::Update` streams a new agent program (up to 256 MiB) to the
  agent, which stages it, runs it with `--version`, swaps it in (Unix: rename over the
  program; Windows: the running one is renamed to `.old`), answers `FileResult` and restarts
  itself (`COMETDESK_AGENT_RESTARTED` makes the successor wait for the instance port). The
  GUI sends the program from `agents/` (`setup::agent_binary`, by the hello's `os` and `arch`)
  when the agent's protocol is older, or the same with an older `version`: **bump the crate
  version to roll out a change**. One attempt per agent version per run of the GUI.
  When the protocols differ and both are ≥ 8 (`UPDATE_PROTOCOL`) the session is *limited*:
  `Event::Mismatch`, and only `Purpose::Update` transfers and pings are served, so a newer
  GUI can still update an older agent across a protocol bump. For that to keep working,
  never reorder `Msg` (append only) and keep `Hello` as it is. Anyone paired can replace the
  agent program this way: the pairing code is the trust.
- **Reboot notice**: an `Info` whose `uptime_secs` is lower than the previous one raises a
  "Target restarted" notification (sleep does not reset it).
- **macOS agent** (development targets): layouts listed and switched through the Text Input
  Sources API; CPU (`ps`), memory (`vm_stat`), uptime (`kern.boottime`) and battery
  (`pmset`).

### Fallback for macOS targets / no network: FunctionFS

A userspace daemon on the Comet could present either a vendor bulk interface (WinUSB
auto-binds via Microsoft OS descriptors; libusb on Linux/macOS) or a userspace CDC-ACM
port, and relay it to the host. This needs our own persistent software on the Comet and
has to coexist with `kvmd-otg`'s gadget management. Last resort.

Goal: a shared clipboard between the controlling host and the target, carried
*through the Comet*, so plain Cmd/Ctrl+C and Cmd/Ctrl+V work in both directions like
TeamViewer. It needs a small agent on the target; "type clipboard" and "OCR copy"
(PLAN.md §3, §8) stay as the no-agent fallback.

## 1. Path

```
cometdesk-gui ──wss /api/serial/ws?dev=/dev/ttyGS0──▶ kvmd (raw fd bridge)
                                                   │ /dev/ttyGS0 (USB gadget serial, acm.usb0)
                                                   ▼ USB cable to target
target: COM3 / /dev/ttyACM0 / /dev/cu.usbmodem*  ◀──▶  cometdesk-agent
```

Why this path:
- **The bridge already exists.** `GET /api/serial/ws` (`kvmd/apps/kvmd/api/serial.py`)
  opens any path that starts with `/dev/`, puts it in raw 8N1 mode (no echo, no CR/LF
  translation, `oflag=0`, no flow control by default) and relays binary WebSocket frames
  both ways. It is meant for USB-serial adapters plugged into the Comet, but nothing
  restricts it to them. No firmware change is needed for the data path.
- **The target needs no driver.** CDC-ACM is built in on Windows 10+ (`usbser.sys`),
  macOS and Linux (`cdc_acm`).
- **No network path between host and target is needed.** That's the advantage over
  established network tools (§5).

## 2. The blocker: turning on the serial gadget

The firmware can present a USB serial port to the target (`add_serial()` creates
`acm.usb0` in `kvmd/apps/otg/__init__.py`), but:
- it is **not** in the API's toggle list. `POST /api/system/otg_functions`
  (`_OTG_FUNC_MAP` in `kvmd/apps/kvmd/api/system.py`) only covers keyboard, mouse,
  mouse_alt, cdrom, flash, mic, camera and MTP;
- it is enabled only by config: `otg.devices.serial.enabled: true` (and `start: true`).

Options:

| | How | Cost |
|---|---|---|
| **A. SSH config override** | On the Comet, write `/etc/kvmd/override.d/90-cometdesk-rs-serial.yaml` with `otg: {devices: {serial: {enabled: true, start: true}}}`, then restart `kvmd-otg` and `kvmd` | Needs SSH as root. The USB device re-enumerates (keyboard/mouse drop for a second). May not survive firmware upgrades: check. |
| **B. Upstream request** | Ask GL.iNet (issue/PR on gl-inet/glkvm) to add `"enable_serial": ("otg/devices/serial/start", False, "acm.usb0")` to `_OTG_FUNC_MAP`, with `serial.enabled: true` by default | Clean, toggleable from our app via the API, but on their schedule |

**Do not** use PiKVM's `ENABLE_OTG_SERIAL` boot option (`scripts/kvmd-bootconfig`). It
also starts a **root login getty on `ttyGS0`**, which would hand a root shell on the
Comet to anyone at the target, and it would fight our agent for the port.

## 3. Agent (`cometdesk-agent`, same repo, permissive license)

- Small Rust binary per OS. Clipboard via `arboard` plus change notifications:
  - Windows: `AddClipboardFormatListener`;
  - macOS: `NSPasteboard.changeCount` polling (~250 ms);
  - Linux: X11 XFixes selection events, or Wayland `wl-clipboard`/`ext-data-control`.
- Port discovery: enumerate serial ports (`serialport` crate), keep those whose USB
  VID/PID/product string match the Comet gadget, and confirm with a `HELLO` handshake.
  Never write to an unrecognized port, since it could be a real modem.
- Linux: ship a udev rule with `ENV{ID_MM_DEVICE_IGNORE}="1"` for the gadget, otherwise
  ModemManager probes the new `ttyACM` with AT commands.
- Runs as the logged-in user: a user-session clipboard isn't reachable from a service.
  Autostart via the Startup folder, a LaunchAgent, or XDG autostart.

## 4. Protocol

- **Framing**: COBS-encoded frames separated by `0x00`, so either side resyncs after
  junk or a half-written frame (port reopened mid-transfer). CRC32 per frame.
- **Messages** (`postcard`-encoded): `Hello{version, caps, os}`, `Auth` (§6),
  `Offer{id, formats, sizes, sha256}`, `Request{id, format}`,
  `Data{id, offset, bytes}`, `Done{id}`, `Ping`/`Pong`.
- **Flow**:
  - On a local clipboard change, send `Offer`.
  - Text and small images (< 1 MB) are pushed straight after the offer, so plain paste
    works instantly.
  - Larger items are fetched on `Request`. This could later use delayed rendering
    (Windows delayed clipboard rendering, `NSPasteboardItemDataProvider`, X11 selection
    ownership).
- **Loop prevention**: each side remembers the hash of what it last applied and doesn't
  re-offer it.
- **Formats**: v1 `text/plain;charset=utf-8` and `image/png`. Files later; until then,
  drag-and-drop to MSD (PLAN.md §8) covers files.
- **Throughput**: CDC-ACM over USB 2.0 bulk endpoints isn't limited by the baud value;
  expect around 1 MB/s or more, which is plenty for text and screenshots. Measure it.

## 5. Established tools: can we reuse one instead of writing an agent?

| Tool | Transport | Usable here? |
|---|---|---|
| Deskflow / Input Leap / Barrier / Synergy | TCP between machines | Only with a direct network path between host and target, which bypasses the Comet. Also shares keyboard and mouse, which conflicts with the KVM. |
| KDE Connect, ClipCascade, CopyQ sync | Network / cloud | Same: network path required, not through the Comet |
| SPICE `spice-vdagent` | virtio-serial, clipboard protocol designed for exactly this | The protocol fits, but the agents expect a virtio port. The Windows agent is tied to virtio-serial, and the Linux daemon may take a port path, unverified against a tty. Not practical. |
| **OSC 52** escape sequence | Terminal output | **Yes, as a bonus**: for Linux targets used through a serial console, our serial terminal can honor OSC 52 (target `tmux`/`vim` → host clipboard) and bracketed paste. Covers shell-only targets with no agent. |

Conclusion: no established clipboard tool speaks "clipboard over a USB serial port",
so the bridge needs our own agent. OSC 52 is the only standard worth implementing.

## 6. Security

- **Host side**: `/api/serial/ws` needs the device admin login, which already grants
  full keyboard control of the target, so reading the clipboard adds no new power.
- **Target side**: any local process can open the COM port, so it could impersonate the
  agent and receive the host's clipboard, including passwords. Mitigations:
  - **Pairing**: when the agent is installed it shows a code, which the user enters in
    the GUI once. From then on, every session runs Noise `NNpsk0` (`snow` crate) over the
    framed link, so content is encrypted and authenticated end to end, and the Comet
    only relays ciphertext.
  - **Per-device direction policy**: host→target, target→host or both, plus a "pause
    sync" toggle in the toolbar.
  - **Respect "concealed" clipboard markers**: never sync content flagged by password
    managers (`org.nspasteboard.ConcealedType` / `TransientType` on macOS,
    `ExcludeClipboardContentFromMonitorProcessing` / `CanIncludeInClipboardHistory=0`
    on Windows, `x-kde-passwordManagerHint` on Linux).
- **One client at a time**: the bridge has no lock, so two WebSocket clients on
  `ttyGS0` would split the incoming bytes between them. The GUI must assume it's the
  only reader, detect a second one (`Hello` from an unexpected session), and warn.
- Firmware note, not ours to fix: the bridge's only path check is `startswith("/dev/")`.

## 7. Verification (before building anything)

All steps need device access (SSH or an authenticated session).

1. On the Comet: `ls /dev/ttyGS*`, check whether `acm.usb0` already exists under
   `/sys/kernel/config/usb_gadget/*/functions/`, and read `/etc/kvmd/override.yaml` and
   `override.d/`.
2. Enable the gadget (option A) and confirm a COM port appears on the target
   (Device Manager / `ls /dev/ttyACM*` / `ls /dev/cu.usbmodem*`).
3. Loopback: connect a WebSocket client (e.g. `websocat`) to
   `wss://<comet>/api/serial/ws?dev=/dev/ttyGS0` with the auth cookie, open the port on
   the target with PuTTY/`screen`, and check bytes flow both ways, including `0x00` and
   CR/LF unchanged.
4. Check the USB re-enumeration impact on keyboard/mouse in BIOS and on the OS, and
   whether the override survives a reboot and a firmware upgrade.
5. If 1–4 pass, file option B upstream and build the agent: text first, then images.
