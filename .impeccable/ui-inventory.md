# CometDesk: UI inventory

Native macOS app that shows and controls a remote computer through a GL.iNet Comet KVM. The user is a homelab / IT power user. Sessions are long and stream-first: the remote screen is the product, and the app's UI must stay out of its way.

Constraints for the designer:
- Built with egui (immediate-mode Rust GUI) on wgpu. No HTML/CSS: the design must be expressible as colours, spacing, radii, fonts, and simple shapes. No blur, no gradients, no custom shaders on UI. Custom icons must be vector shapes or bitmaps we draw by hand; the default font has no arrow or symbol glyphs (e.g. "▾" renders as an empty box).
- Default window 1280×832, resizable, fullscreen supported. Dark theme only. The app name "CometDesk" and the logo (monitor with cursor, blue) must stay.
- Power-first: the UI only redraws on input, so no continuous animation.
- Keep every function below reachable. Yellow (#FFD21F) is the "next action" colour. Status colours (green OK, orange warning, red error) must not overlap with the six per-device identity colours.

Current palette: ground #0B0B09, panel #12120F, card #1A1A15, hover #24241C, rule #3A392E, text #F4F1E3, muted #A8A48C, yellow #FFD21F, ok #3FD38A, warn #FF8F2B, error #FF5A52.

---

## 1. Connect screen (shown when no session is open)

| Element | What it does |
|---|---|
| Logo + "CometDesk" + tagline "Remote console for GL.iNet KVM" | Branding. |
| **Add device** button (header, right) | Opens the Add device form. Hidden while the form is open. |
| Busy card (spinner + text, e.g. "Checking the saved password…") | Shown while connecting or reading the saved password; disables Connect. |
| Error banner (red) | Last connect failure or "Could not save devices". |
| **Device row** (one per saved device) | Square coloured badge with the device's initial (colour is derived from the host, six colours), name (large), reachability, address line `user@host · target OS`. |
| Reachability indicator | "Reachable" (green dot), "Not reachable" (orange dot) or "Checking…". Probed over TCP every 30 s. |
| **Connect** button | Connects to that device. Yellow only on the first reachable device; neutral on the others. Disabled while busy. |
| **•••** menu (per row) | Opens a menu: Edit… and Remove… |
| Edit… | Opens the form pre-filled (password field shows "unchanged"). |
| Remove… | Opens a confirm dialog (see 7) because the saved password goes with the device. |
| Empty state card "Add your Comet" | Shown when no device is saved. Explains the usual address (192.168.200.1, over USB or the Comet's own Wi-Fi) with an **Add device** button. |

### Add / Edit device form (inline card)
Title "Add device" or "Edit device". Fields: **Name** (hint "Office Comet"), **Address** (hint "192.168.200.1 or host:port"), **User** (default admin), **Password**, **Target** dropdown (Linux / Windows / macOS / BIOS-other: which OS the remote machine runs; sets shortcut and paste behaviour), checkbox **Remember password in the keychain**, buttons **Save and connect** (primary, enabled when valid) and **Cancel**.

### Prompts (modal dialogs on the connect screen)
- **Password for user@host**: password field, "Remember in the keychain" checkbox, red banner "The saved password was rejected.", buttons Cancel / Connect.
- **2FA code for <device>**: "The device has two-factor authentication enabled." One code field (hint 123456), Cancel / Connect.

---

## 2. Console (session open): the remote video fills the window

### 2a. Top bar (hidden in fullscreen, shown on hover at the top edge after a 350 ms dwell, or with Right Cmd+T)
Left side:
| Element | What it does |
|---|---|
| **‹ Devices** | Ends the session and returns to the connect screen. |
| Device badge + name | Which machine this is (same badge colour as the connect row). |
| Status (dot + word, fixed width so controls never shift) | "Connected" (green); amber: "Reconnecting…", "Paused (window hidden)", "No signal", "Keyboard/mouse offline"; red: "Disconnected by the device", "Closed". Hover tooltip explains the agent state. |
| **Keys** (dropdown) | Sends key combos to the target: Ctrl+Alt+Del, Win/Super, Alt+Tab, Alt+F4, Ctrl+Esc, Print Screen, Ctrl+Alt+F1 (Linux TTY), Ctrl+Alt+F2, Cmd+Space (macOS), Cmd+Tab (macOS); submenu **Function keys** F1–F12. |
| **Paste** | Types or inserts the Mac clipboard into the target (through the agent in one step, otherwise key by key). Right Cmd+V. Long or multi-line text asks for confirmation first (see 7). |
| **Capture** (dropdown) | Save device snapshot (JPEG to ~/Downloads); Save exact screenshot (agent, all monitors); Copy region as image (drag a rectangle on the video); Copy screenshot as image (main monitor); Copy screen text (OCR on this Mac). The agent-based items are greyed out with a tooltip when the agent isn't connected. |
| **Power** (dropdown, only on devices with ATX power control) | Power on, Shut down, Force off, Reset, Press power button; each asks for confirmation. |

Right side:
| Element | What it does |
|---|---|
| **Actions** | Opens the action search (2c). Right Cmd+K. |
| **Panel** | Toggles the side panel (2b). |
| **Fullscreen** / **Exit fullscreen** | Toggles fullscreen. Right Cmd+F. |

Under the bar: a 1 px line in the connection-state colour (the "signal edge"). In fullscreen with the bar hidden and the state not OK, a floating status chip appears at top centre.

### 2b. Side panel (320 px, over the right edge of the video), tabs Target / Video / Settings, plus Close
Inside the panel, groups expand in place (collapsible sections); the same groups are popup submenus where reused elsewhere.

**Target tab**
- Banners (amber): "The target is locked (lock screen)." / "Clipboard needs attention: open Clipboard below."
- **Clipboard** section (title is tinted green when synced, amber when it needs attention): state line "Clipboard: synced / paused / off / needs repair / not connected / setting up…"; "Comet: bridge ready <firmware>" or a health problem in amber; a repair button when the Comet's bridge needs installing; "Target: connected to <host> (<os>)"; radio group direction **Both ways / Only to the target / Only from the target**; checkbox **Pause**; for Linux targets: "Insert clipboard pastes with:" radios **Auto (Ctrl+Shift+V in terminals) / Ctrl+V / Ctrl+Shift+V**; buttons **Set up the target again…** and **Forget pairing**. When not set up: text "Shares the clipboard with the target." and **Set up the target…**.
- **Send file…**: pick files to copy to the target's Downloads folder (files can also be dropped on the window). Only when the agent is connected.
- **Browse files…**: opens the file browser window (3). Only when the agent is connected.
- **Session** section: Lock, Sleep, Log off…, Restart…, Shut down… (the last three ask for confirmation).
- **System info** section (title shows "· locked" in amber when locked): two-column list: Host, OS, User, Session, Keyboard, Layouts, Input method, Focus, Display (resolution, position, scale, "main"), Uptime, Battery, CPU, Memory, Disk; per-display submenus **Display N**: Resolution list, **Make this the main display**, **Match the EDID: <preset>**; **Switch the target's layout** (keyboard layout of the target); **Volume** slider and **Mute** checkbox.
- **Media** section (only on devices with virtual media; title shows "Media 42%" while uploading and "Media ●" when an image is mounted): states "Reading the device…", "Virtual media is off on this device", "Mounted: <image> (CD-ROM / USB drive)" with **Eject**, "Nothing mounted", "No images on the device"; image list "<name> (size)" with **Mount as CD-ROM**, **Mount as USB drive**, **Delete from device** (then "Really delete <name>"); **Upload image…** (or drop a file); upload progress with **Cancel upload**; "<n> free".

**Video tab**
- **This viewer**: "Video source" radios Direct (LAN, lowest latency) / Remote (WebRTC with error correction) / Remote (WebRTC, Janus); checkbox **Play audio**; **Audio output** (System default or a named device).
- **Whole device** (orange heading, "Changes apply to every viewer, after confirming"): sections **Streaming protocol** (WebRTC with error correction / WebRTC / GL native (no Janus) / Direct H.264), **Codec** (H.264 / H.265), **Bandwidth saving** (Smart / Normal), **Latency mode** (Lowest latency / Smooth display), **Orientation** (0°/90°/180°/270°), **Display resolution** (presets, "Custom", with "(no audio)" notes).
- Encoder summary line "Encoder: N fps, N kbit/s, keyframe every N frames", **Frame rate** field with chips 30 / 60, **Bitrate** field with chips Low / Medium / High / Max, **Apply…** (asks for confirmation).

**Settings tab**
- **Target OS: <os>** (Windows / Linux / macOS / BIOS-other): changes shortcut and paste behaviour; saved on the device.
- **Keyboard layout: <layout>**: list of layouts, marks "(on target)".
- Checkbox **Show stats** (Right Cmd+S): toggles the stats readout (2d).
- **Device web interface…**: opens the Comet's own web page in the browser.

### 2c. Action search (modal, Right Cmd+K, also the **Actions** button)
Search field "Search actions…" and a scrolling list. Every action is listed with a right-aligned shortcut or group name (Keys / Power / Capture / Files / Viewer). Up/Down moves, Enter runs, Esc closes, mouse click runs. Actions: Paste the clipboard (Right Cmd V); Send <each key combo>; Power on / Shut down / Force power off / Reset / Press power button (if ATX); Save device snapshot / Save exact screenshot / Copy region as image / Copy screenshot as image / Copy screen text; Toggle fullscreen (Right Cmd F); Show stats (Right Cmd S); open Panel tabs; Send file; Browse files; Device web interface; Disconnect. Shows "No action matches." when empty.

### 2d. Overlays on the video
| Element | What it does |
|---|---|
| Centre message (large, muted) | "No signal: the target is off or asleep", "Reconnecting…", "Resuming…", "Waiting for video…". |
| Stats readout (bottom right, monospace) | Live fps, bitrate, latency etc. when Show stats is on. |
| Typing progress (bottom centre) | Progress bar "Typing 12/240" with **Cancel** while Paste types key by key. |
| Notice toast (top centre) | One-line messages: "Typed the clipboard", "Copied N characters of screen text", "Display 2 is now the main display", failures with the reason. |
| File transfer card | "<name> · 1.2 / 4.0 MB" with **Cancel** while a file is being sent. |
| Region picker | Dimmed video with the hint "Drag over the part of the screen to copy · Esc cancels"; drag draws a rectangle. |
| Remote cursor and keyboard | All mouse and keyboard input goes to the target; Right Cmd is the host key (Right Cmd+V/F/S/T/K). |

---

## 3. File browser (separate window "Files on the target")
Buttons **Home**, **Up**, **Drives**; checkbox **Hidden**; **Filter** field; list of folders (click opens them, shown as "name/") and files with sizes (B/KB/MB/GB); **Download** per file (saves to this Mac); note "More entries than fit: use the filter or open a subfolder".

## 4. Set up the target (modal wizard)
Title "Set up the target". "The target runs:" Linux / Windows / macOS selector. Steps: "1. Open a terminal / Run dialog on the target" with a button **Open a terminal (Ctrl+Alt+T)** or **Open the Run dialog (Win+R)**; "2. Run this command there…" with a read-only command box, **Run it on the target** (types it and presses Enter), **Copy**; status line; green "The agent is connected: the clipboard is shared now."; amber warning if the Comet lacks an agent build; **Close** or **Done**. Unsupported OS shows a manual `cometdesk-agent pair <code>` line.

## 5. Confirm dialogs (modal, same layout: question title, consequence text, **Cancel**, **Continue**)
Type the clipboard (long / multi-line text); ATX power actions and the power-button press; session Log off / Restart / Shut down; install the clipboard bridge on the Comet; switch video mode; set encoder frame rate / bitrate / keyframe; switch codec; switch bandwidth-saving mode; switch latency mode; rotate the picture; present the target a different display resolution. Each explains that device-wide changes affect every viewer, including the web interface.

## 6. Remove device dialog
"Remove <name>?" with a note that its saved password is deleted too; **Cancel** / **Remove**.

## 7. System-level
macOS Touch ID / keychain prompt before saved passwords are read (release builds). Window title "cometdesk — <device name>". Dock icon (logo). Native notifications for target messages while the window is in the background. Menu bar hidden in fullscreen.

## 8. Known weaknesses to fix in the new design
- The connect screen leaves most of the window empty with one device.
- Dropdowns in the bar (Keys, Capture, Power) look the same as plain buttons: no affordance.
- The side panel is a floating box; its tabs mostly hold nested, expanding lists of radios.
- Buttons in the panel vary in width and alignment.
- One typeface only (egui default); no bold weight is available, so hierarchy relies on size and colour.
