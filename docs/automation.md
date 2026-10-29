# Automation

Scripts and agents can drive the GUI over a local control socket.

```
COMETDESK_CONTROL=1 COMETDESK_AUTOCONNECT=1 cargo run -p cometdesk-gui   # or COMETDESK_CONTROL=/path/to.sock
tools/cometdeskctl.py status
```

The socket (`<config dir>/cometdesk/control.sock` for `1`, mode 0600, Unix only) gives full
control of the target's keyboard and mouse: it is off unless `COMETDESK_CONTROL` is set.

Protocol: one JSON object per line in, one per line out. An optional `"id"` is echoed.
Replies are `{"ok":true,...}` or `{"ok":false,"error":"..."}`.

| cmd | args | result |
|---|---|---|
| `status` | | screen (`devices`/`connecting`/`console`), window and video size, connection, typing progress, last notice |
| `devices` | | saved device names |
| `connect` / `disconnect` / `quit` | `name` (name or host) | connect only starts; poll `status` |
| `screenshot` | `path`? | PNG of the app window (video + UI) |
| `snapshot` | `path`? | the device's JPEG of the target |
| `ocr` | `path`? | snapshot plus `text` (macOS Vision) |
| `key` | `code` (W3C, e.g. `KeyA`), `action`: `tap`/`down`/`up` | |
| `combo` | `keys`: [codes] | press in order, release reversed |
| `type` | `text` | typed (or inserted by the agent); progress in `status.console.typing` |
| `mouse_move`, `click` | `x`,`y` in 0..1 of the target, or `px`,`py` in target pixels; `button`, `count` | |
| `mouse_button` | `button`, `action`: `down`/`up` | |
| `wheel` | `dx`, `dy` | |
| `release_all` | | |
| `ui_click`, `ui_text` | `x`,`y` window points; `text` | drives the app's own UI |

Python: `from cometdeskctl import Client; c = Client(); c.call("click", x=.5, y=.5)`.

Notes: `screenshot` needs the window visible (a hidden window never draws, so it times out).
Not yet covered: an accessibility tree of the egui UI, and returning pixels of the decoded
target video (macOS frames are zero-copy IOSurfaces); use `snapshot` for the target.
