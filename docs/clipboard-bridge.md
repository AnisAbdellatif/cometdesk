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
target: glkvm-agent ──TCP──▶ 192.168.5.1 (Comet usb0) ──NAT out eth0──▶ host:47474 (glkvm-gui listens)
```

- No Comet code changes: one config override, then the existing `kvmd-rndis`.
- The **agent connects out** to the host; the host listens on one port, firewalled to
  the Comet's LAN IP. The host address and port come from pairing (§6).
- TCP gives reliability and ordering, so §4's COBS/CRC framing reduces to
  length-prefixed frames inside the Noise session.

Costs and caveats:
- Enabling it: write `/etc/kvmd/override.d/90-glkvm-rs-rndis.yaml` with
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

- Wrote `/etc/kvmd/override.d/90-glkvm-rs-rndis.yaml` (ethernet `enabled: true`,
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
- Undo: `rm /etc/kvmd/override.d/90-glkvm-rs-rndis.yaml; kvmd-rndis stop;
  /etc/init.d/S50kvmd-otg restart`.

### Removing the limits: a relay on the Comet (`glkvm-bridge`)

The NAT path above has three limits:
- the host must be on the Comet's LAN;
- macOS targets can't use RNDIS;
- the target needs routing changes.

All three go away if **both sides connect *to* the Comet** and a small relay on the
Comet pairs them:

```
target: glkvm-agent ──USB link──▶ glkvm-bridge (on Comet) ◀──wss /glkvm-bridge/ws── glkvm-gui
                                                               (any path that reaches the
                                                                Comet's web UI)
```

- **True remote for free.** The GUI side is just another WebSocket path on the
  Comet's nginx. Every way of reaching the web UI already tunnels nginx: LAN,
  Tailscale/ZeroTier/NetBird on the Comet, Cloudflare tunnel, GL cloud, self-hosted
  glkvm-cloud. So the clipboard works wherever video and HID work. The host never
  needs to be reachable from the Comet, and nothing is NATed.
- **Auth for free.** It ships as a kvmd *extra*, GL's own mechanism for `/pion/ws`,
  `/lws/ws` and `/api/media/ws`: `/usr/share/kvmd/extras/glkvm-bridge/` with
  `manifest.yaml`, `nginx.ctx-http.conf` (upstream to a unix socket) and
  `nginx.ctx-server.conf` (`location /glkvm-bridge/ws`, with `loc-proxy.conf` and
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
- Started by `/etc/init.d/S99glkvm-bridge`.
- Exposes `GET /glkvm-bridge/ws` (GUI side) and the USB-side listener, plus
  `GET /glkvm-bridge/status` (version, agent connected, link type) so the GUI can detect
  whether the bridge is installed and current.

**Install / update / remove from the GUI** ("Enable clipboard bridge"):
- Over SSH, with the user's key or password, entered once and not stored unless asked.
- Copies the binary, extra, init script and OTG override, restarts `S50kvmd-otg`
  (warn: USB re-enumerates) and `S99kvmd-nginx`. Idempotent and versioned.
- Firmware upgrades may wipe the overlay (unverified): the GUI sees a 404 on
  `/glkvm-bridge/status` and offers to reinstall.

**Remote paths to verify**:
- [ ] LAN `https://<comet>/glkvm-bridge/ws`
- [ ] Tailscale on the Comet
- [ ] Cloudflare tunnel
- [ ] GL cloud (glkvm.com device URL): does its proxy pass arbitrary paths and
  WebSockets? `/janus/ws`/`/pion/ws` work there, so likely.
- [ ] self-hosted glkvm-cloud (open source, check its proxy rules)

### Prototype results (2026-10-28, `bridge/`)

Installed with `bridge/install.sh` (removed by `bridge/uninstall.sh`):
- `glkvm_bridge.py` (Python/aiohttp, run by `/etc/init.d/S99glkvm-bridge`);
- the kvmd extra `glkvm-bridge`;
- `udhcpd` with no router or DNS;
- an `iptables` chain allowing only DHCP, ICMP and TCP 47474 in from `usb0`, plus
  `FORWARD -i usb0 -j DROP`; GL's NAT removed.

- nginx without a login: `401` on `/glkvm-bridge/status` and on the WebSocket handshake.
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

Not yet done: remote paths (checklist above), surviving a Comet reboot, a real agent,
Noise E2E, a Rust relay.

**Upstream asks** (would remove the SSH install step): ship the kernel with
`CONFIG_USB_CONFIGFS_ACM`/`NCM`, add serial/ethernet to `_OTG_FUNC_MAP`, and allow
user extras that survive upgrades.

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
glkvm-gui ──wss /api/serial/ws?dev=/dev/ttyGS0──▶ kvmd (raw fd bridge)
                                                   │ /dev/ttyGS0 (USB gadget serial, acm.usb0)
                                                   ▼ USB cable to target
target: COM3 / /dev/ttyACM0 / /dev/cu.usbmodem*  ◀──▶  glkvm-agent
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
| **A. SSH config override** | On the Comet, write `/etc/kvmd/override.d/90-glkvm-rs-serial.yaml` with `otg: {devices: {serial: {enabled: true, start: true}}}`, then restart `kvmd-otg` and `kvmd` | Needs SSH as root. The USB device re-enumerates (keyboard/mouse drop for a second). May not survive firmware upgrades: check. |
| **B. Upstream request** | Ask GL.iNet (issue/PR on gl-inet/glkvm) to add `"enable_serial": ("otg/devices/serial/start", False, "acm.usb0")` to `_OTG_FUNC_MAP`, with `serial.enabled: true` by default | Clean, toggleable from our app via the API, but on their schedule |

**Do not** use PiKVM's `ENABLE_OTG_SERIAL` boot option (`scripts/kvmd-bootconfig`). It
also starts a **root login getty on `ttyGS0`**, which would hand a root shell on the
Comet to anyone at the target, and it would fight our agent for the port.

## 3. Agent (`glkvm-agent`, same repo, permissive license)

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
