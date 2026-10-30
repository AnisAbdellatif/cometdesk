#!/usr/bin/env python3
"""End-to-end smoke test against a real device: launch, connect, video, input, screenshots.

  tools/smoke.py [device]     # default device: the first saved one (or COMETDESK_AUTOCONNECT's)
Moves the target's mouse and types a harmless key (Shift); exits 1 on the first failed check.
"""
import os, sys, tempfile
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cometdeskctl import CONDITIONS, launch

failed = []
def check(name, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + name, detail if not ok else "")
    if not ok:
        failed.append(name)

c = launch()
try:
    dev = sys.argv[1] if len(sys.argv) > 1 else c.call("devices")["devices"][0]
    check("connect", c.call("connect", name=dev).get("ok"))
    c.wait(CONDITIONS["connected"], 30)
    st = c.wait(CONDITIONS["video"], 30)
    check("video", st["video"]["width"] > 0, st)
    out = tempfile.mkdtemp(prefix="cometdesk-smoke-")
    r = c.call("screenshot", path=f"{out}/window.png")
    check("window screenshot", r.get("ok") and os.path.getsize(r["path"]) > 10_000, r)
    r = c.call("snapshot", path=f"{out}/target.jpg")
    check("device snapshot", r.get("ok") and r["bytes"] > 10_000, r)
    for cmd, args in [("click", {"x": .5, "y": .5}), ("key", {"code": "ShiftLeft"}),
                      ("wheel", {"dy": 1}), ("release_all", {})]:
        r = c.call(cmd, **args)
        check(cmd, r.get("ok"), r)
    check("bad command rejected", not c.call("bogus").get("ok"))
    print("artifacts:", out)
finally:
    c.call("quit")
sys.exit(1 if failed else 0)
