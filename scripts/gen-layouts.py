#!/usr/bin/env python3
"""Generates crates/glkvm/src/layout/data.rs: keyboard layouts for typing text over HID.

Source: Unicode CLDR release-43 keyboard data (keyboards/windows, keyboards/osx), under the
Unicode License v3 (copied to crates/glkvm/src/layout/CLDR-LICENSE). Windows layouts also
serve Linux targets: for the keys we type, xkb's layouts match them.

For each layout and each character it records the physical key (as a KeyboardEvent.code,
which is what the device's HID path takes) and the modifiers: Shift and/or AltGr (Option on
macOS). Characters that are dead keys on the layout get a following Space; characters that
only exist as dead-key compositions (e.g. â = ^ then a) get both strokes.

Usage: python3 scripts/gen-layouts.py   (downloads into target/cldr-cache)
"""

import os
import re
import sys
import urllib.request
import xml.etree.ElementTree as ET

TAG = "release-43"
BASE = f"https://raw.githubusercontent.com/unicode-org/cldr/{TAG}/keyboards"
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
CACHE = os.path.join(ROOT, "target", "cldr-cache")
OUT_DIR = os.path.join(ROOT, "crates", "glkvm", "src", "layout")

# Device keymap name (kvmd) -> CLDR locale of the standard layout.
LAYOUTS = {
    "en-us": "en", "en-gb": "en-GB", "de": "de", "de-ch": "de-CH", "fr": "fr",
    "fr-be": "fr-BE", "fr-ch": "fr-CH", "fr-ca": "fr-CA", "es": "es", "it": "it",
    "pt": "pt-PT", "pt-br": "pt", "nl": "nl", "da": "da", "no": "nb", "sv": "sv",
    "fi": "fi", "pl": "pl", "cz": "cs", "hu": "hu", "tr": "tr", "ru": "ru", "is": "is",
    "et": "et", "sl": "sl", "lt": "lt", "lv": "lv",
}
PLATFORMS = {"windows": "Windows", "osx": "Mac"}

ISO_CODES = {"E00": "Backquote", "E11": "Minus", "E12": "Equal", "D11": "BracketLeft",
             "D12": "BracketRight", "D13": "Backslash", "C10": "Semicolon", "C11": "Quote",
             "C12": "Backslash", "B00": "IntlBackslash", "B08": "Comma", "B09": "Period",
             "B10": "Slash", "A03": "Space"}
for i, d in enumerate("1234567890"):
    ISO_CODES[f"E{i + 1:02d}"] = f"Digit{d}"
for row, letters in (("D", "QWERTYUIOP"), ("C", "ASDFGHJKL"), ("B", "ZXCVBNM")):
    for i, l in enumerate(letters):
        ISO_CODES[f"{row}{i + 1:02d}"] = f"Key{l}"

SHIFT, ALTGR = 1, 2


def fetch(platform: str, locale: str) -> str:
    name = f"{locale}-t-k0-{platform}.xml"
    path = os.path.join(CACHE, name)
    if not os.path.exists(path):
        os.makedirs(CACHE, exist_ok=True)
        with urllib.request.urlopen(f"{BASE}/{platform}/{name}") as r:
            data = r.read()
        with open(path, "wb") as f:
            f.write(data)
    with open(path, encoding="utf-8") as f:
        return f.read()


def unescape(s: str) -> str:
    return re.sub(r"\\u\{([0-9A-Fa-f]+)\}", lambda m: chr(int(m.group(1), 16)), s)


def level(modifiers: str) -> int | None:
    """Modifier bits for a keyMap, from its simplest alternative we can press; None to skip it.

    Alternatives are space-separated, `?` marks optional modifiers, and macOS names sides
    (`shiftL`, `optR`). AltGr is `altR` or `ctrl+alt` on Windows, Option on macOS."""
    best = None
    for alt in (modifiers or "").split(" "):
        mods, rest = 0, set()
        for m in alt.split("+"):
            if not m or m.endswith("?"):
                continue
            if m in ("shift", "shiftL", "shiftR"):
                mods |= SHIFT
            elif m in ("altR", "opt", "optL", "optR"):
                rest.add("altgr")
            else:
                rest.add(m)
        if rest in ({"altgr"}, {"ctrl", "alt"}, {"alt"}):
            mods |= ALTGR
            rest = set()
        if not rest and (best is None or bin(mods).count("1") < bin(best).count("1")):
            best = mods
    return best


def parse(xml: str):
    root = ET.fromstring(re.sub(r"<!DOCTYPE[^>]*>", "", xml))
    direct: dict[str, tuple[str, int]] = {}
    for km in root.iter("keyMap"):
        mods = level(km.get("modifiers", ""))
        if mods is None:
            continue
        for m in km.iter("map"):
            iso, to = m.get("iso"), unescape(m.get("to", ""))
            code = ISO_CODES.get(iso)
            if code is None or len(to) != 1:
                continue
            best = direct.get(to)
            if best is None or bin(mods).count("1") < bin(best[1]).count("1"):
                direct[to] = (code, mods)
    dead, composed = set(), {}
    for tr in root.iter("transform"):
        frm, to = unescape(tr.get("from", "")), unescape(tr.get("to", ""))
        if len(frm) == 2 and len(to) == 1:
            dead.add(frm[0])
            composed.setdefault(to, frm)
    table: dict[str, list[tuple[str, int]]] = {}
    for ch, stroke in direct.items():
        table[ch] = [stroke, ("Space", 0)] if ch in dead else [stroke]
    for ch, (d, base) in composed.items():
        if ch not in table and d in direct and base in direct:
            table[ch] = [direct[d], direct[base]]
    return table


def main() -> None:
    codes: list[str] = []
    entries = []
    for name, locale in LAYOUTS.items():
        for platform, pname in PLATFORMS.items():
            try:
                table = parse(fetch(platform, locale))
            except urllib.error.HTTPError:
                print(f"skip {platform}/{locale}", file=sys.stderr)
                continue
            rows = []
            for ch in sorted(table):
                strokes = []
                for code, mods in table[ch]:
                    if code not in codes:
                        codes.append(code)
                    strokes.append(f"S({codes.index(code)}, {mods})")
                rows.append(f"({rust_char(ch)}, &[{', '.join(strokes)}])")
            entries.append((name, pname, rows))
    out = [
        "// @generated by scripts/gen-layouts.py from Unicode CLDR "
        f"{TAG} keyboard data. Do not edit.",
        "// CLDR data: Copyright © Unicode, Inc. Unicode License v3, see CLDR-LICENSE.",
        "",
        "use super::{Platform, S, Table};",
        "",
        f"pub(super) static CODES: [&str; {len(codes)}] = [",
        *[f"    {c!r},".replace("'", '"') for c in codes],
        "];",
        "",
        "#[rustfmt::skip]",
        "pub(super) static LAYOUTS: &[(&str, Platform, Table)] = &[",
    ]
    for name, pname, rows in entries:
        out.append(f'    ("{name}", Platform::{pname}, &[')
        out += [f"        {r}," for r in rows]
        out.append("    ]),")
    out.append("];")
    with open(os.path.join(OUT_DIR, "data.rs"), "w", encoding="utf-8") as f:
        f.write("\n".join(out) + "\n")
    with urllib.request.urlopen(f"https://raw.githubusercontent.com/unicode-org/cldr/{TAG}/unicode-license.txt") as r:
        with open(os.path.join(OUT_DIR, "CLDR-LICENSE"), "wb") as f:
            f.write(r.read())
    print(f"{len(entries)} layouts, {len(codes)} key codes -> {OUT_DIR}/data.rs")


def rust_char(ch: str) -> str:
    """A Rust char literal for `ch`."""
    special = {"'": "'\\''", "\\": "'\\\\'"}
    if ch in special:
        return special[ch]
    if ch.isprintable() and not ch.isspace():
        return f"'{ch}'"
    return f"'\\u{{{ord(ch):x}}}'"


if __name__ == "__main__":
    main()
