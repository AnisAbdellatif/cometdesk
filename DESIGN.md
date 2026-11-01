---
name: CometDesk
description: Native macOS KVM console set in airport-wayfinding language: warm-black ground, signage yellow, a colour line per device.
colors:
  ground: "#0B0B09"
  panel: "#12120F"
  card: "#1A1A15"
  card-hover: "#24241C"
  rule: "#3A392E"
  text: "#F4F1E3"
  muted: "#A8A48C"
  signage-yellow: "#FFD21F"
  signage-yellow-hover: "#FFE262"
  ok: "#3FD38A"
  warn: "#FF8F2B"
  err: "#FF5A52"
  line-blue: "#3FA7FF"
  line-violet: "#A77BFF"
  line-teal: "#19C2C2"
  line-magenta: "#F25FA8"
  line-lime: "#9BD52E"
  line-sand: "#D9B38C"
typography:
  heading:
    fontFamily: "egui default Proportional"
    fontSize: "24px"
  body:
    fontFamily: "egui default Proportional"
    fontSize: "14px"
  small:
    fontFamily: "egui default Proportional"
    fontSize: "12px"
  mono:
    fontFamily: "egui default Monospace"
    fontSize: "13px"
rounded:
  widget: "4px"
  card: "6px"
  window: "8px"
spacing:
  item: "8px"
  button-x: "12px"
  button-y: "6px"
  card-x: "16px"
  card-y: "14px"
components:
  button-primary:
    backgroundColor: "{colors.signage-yellow}"
    textColor: "{colors.ground}"
    rounded: "{rounded.widget}"
    padding: "6px 12px"
  card:
    backgroundColor: "{colors.card}"
    rounded: "{rounded.card}"
    padding: "14px 16px"
---

# Design System: CometDesk

## Overview

**Creative North Star: "The Wayfinding Board"**

A console you use for hours in a dim room, read in glances: warm-black ground, signage-yellow for the one thing to do next, and hard-edged, square-cornered tags like airport signage. The world lends type, palette, density and one signature move; the shell stays standard.

**Signature move: the line.** Every saved device owns one of six identity colours, shown as a square badge with its initial on the connect screen and in the console bar. Status is a state, not an identity, so the six line colours never overlap with yellow, green, orange or red.

**Key Characteristics:**
- Stream first: the video fills the window; the bar carries four everyday actions (Keys, Paste, Power, Capture) and the rest lives in a right-hand panel and a search (Right Cmd + K).
- Flat, square: 4px widgets, 6px cards, 8px windows; no shadows.
- Yellow means "the next action", never status.

## Colors

### Primary
- **Signage Yellow** (#FFD21F): primary buttons, focus and hover edge, selection. Text on it is Ground.
- **Signage Yellow Lift** (#FFE262): hover.

### Neutral
- **Ground** (#0B0B09), **Panel** (#12120F), **Card** (#1A1A15), **Card Hover** (#24241C): warm-black steps. **Rule** (#3A392E): 1px borders. **Text** (#F4F1E3), **Muted** (#A8A48C).

### Status
- **OK** (#3FD38A), **Warn** (#FF8F2B), **Err** (#FF5A52): always with a text label.

### Identity lines
- Six device colours (see frontmatter), assigned by a stable hash of the host.

### Named Rules
**The One Yellow Rule.** Yellow is for the single next action on a screen. **The Separate Channels Rule.** Identity (lines), action (yellow) and state (green/orange/red) never share a colour.

## Typography

egui's bundled stacks; hierarchy by weight and size only. Heading 24, body 14, small 12, mono 13.

## Layout

Stream-first: the video owns the window, locked to its aspect. A slim top bar (back, device badge and name, fixed-width status tag, Keys, Paste, Power, Capture; Actions, Panel and Fullscreen on the right). The panel (Target / Video / Settings tabs, 320px) overlays the stream's right edge. The connect screen is a single 560px column of device rows. Item spacing 8px; control height 28px.

## Elevation & Depth

Flat and tonal; no shadows. Hover raises one tonal step and draws a yellow rule.

## Shapes

Square-leaning: 4px widgets and tags, 6px cards and menus, 8px windows. 1px rules.

## Components

### Buttons
Primary: yellow fill, Ground text, bold. Default: Card with a Rule border. Ghost: no frame until hover.

### Device row
Card with a 36px line badge, name, "Reachable / Not reachable" tag, user@host · OS, then Connect (yellow only on the lead device) and a More menu (Edit, Remove with confirm).

### Console bar
22px badge, name, fixed-width status tag, then actions. Its lower rule is the signal edge (status colour; OK at 40%). In fullscreen it appears after a 350 ms rest on the top edge; a floating tag shows any non-OK state.

### Panel and action search
Panel tabs are selectable labels. The action search is a modal with a single field; Enter runs the first match.

## Do's and Don'ts

### Do:
- **Do** take colours from `theme.rs` constants.
- **Do** keep every feature reachable through the bar, panel or search.
- **Do** pair every status colour with text.

### Don't:
- **Don't** use a line colour for status or yellow for identity.
- **Don't** add shadows, gradients or rounded pills.
- **Don't** put new top-level controls on the bar; they go in the panel and the search.
