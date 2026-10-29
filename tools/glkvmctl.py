#!/usr/bin/env python3
"""Drive a running glkvm GUI over its control socket (start it with GLKVM_CONTROL=1).

  glkvmctl.py status
  glkvmctl.py connect Comet
  glkvmctl.py screenshot [path]        # the app window (video + UI) as PNG
  glkvmctl.py snapshot [path]          # the device's JPEG of the target
  glkvmctl.py ocr                      # text on the target screen (macOS)
  glkvmctl.py type 'hello'             # key presses on the target
  glkvmctl.py key Enter | combo ControlLeft AltLeft Delete
  glkvmctl.py click 0.5 0.5 [left|right]   # target position in 0..1
  glkvmctl.py raw '{"cmd":"wheel","dy":-3}'
Exit status is 1 when the app answers ok=false.
Also usable as a module: `from glkvmctl import Client; Client().call("status")`.
"""
import json, os, socket, sys, time


def default_socket():
    p = os.environ.get("GLKVM_CONTROL", "1")
    if p not in ("1", ""):
        return p
    home = os.path.expanduser("~")
    for c in (f"{home}/Library/Application Support/glkvm/control.sock",
              f"{os.environ.get('XDG_CONFIG_HOME', home + '/.config')}/glkvm/control.sock"):
        if os.path.exists(c):
            return c
    sys.exit("no control socket; start the app with GLKVM_CONTROL=1")


class Client:
    def __init__(self, path=None):
        self.s = socket.socket(socket.AF_UNIX)
        self.s.connect(path or default_socket())
        self.f = self.s.makefile("rw")

    def call(self, cmd, **args):
        self.f.write(json.dumps({"cmd": cmd, **args}) + "\n")
        self.f.flush()
        return json.loads(self.f.readline())

    def wait(self, pred, timeout=30, every=0.5):
        """Polls `status` until pred(status) is truthy."""
        end = time.time() + timeout
        while time.time() < end:
            st = self.call("status")
            if pred(st):
                return st
            time.sleep(every)
        raise TimeoutError("condition not met")


def main(a):
    if not a:
        sys.exit(__doc__)
    c, rest = a[0], a[1:]
    one = lambda k: {k: rest[0]} if rest else {}
    req = {
        "status": lambda: ("status", {}), "devices": lambda: ("devices", {}),
        "connect": lambda: ("connect", {"name": rest[0]}), "disconnect": lambda: ("disconnect", {}),
        "quit": lambda: ("quit", {}), "screenshot": lambda: ("screenshot", one("path")),
        "snapshot": lambda: ("snapshot", one("path")), "ocr": lambda: ("ocr", one("path")),
        "type": lambda: ("type", {"text": " ".join(rest)}),
        "key": lambda: ("key", {"code": rest[0], **({"action": rest[1]} if len(rest) > 1 else {})}),
        "combo": lambda: ("combo", {"keys": rest}),
        "click": lambda: ("click", {**({"x": float(rest[0]), "y": float(rest[1])} if len(rest) > 1 else {}),
                                    **({"button": rest[2]} if len(rest) > 2 else {})}),
        "move": lambda: ("mouse_move", {"x": float(rest[0]), "y": float(rest[1])}),
        "raw": lambda: (json.loads(rest[0]).pop("cmd"), {k: v for k, v in json.loads(rest[0]).items() if k != "cmd"}),
    }.get(c)
    if not req:
        sys.exit(__doc__)
    cmd, args = req()
    r = Client().call(cmd, **args)
    print(json.dumps(r))
    sys.exit(0 if r.get("ok") else 1)


if __name__ == "__main__":
    main(sys.argv[1:])
