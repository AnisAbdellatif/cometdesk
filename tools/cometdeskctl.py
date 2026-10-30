#!/usr/bin/env python3
"""Drive a running cometdesk GUI over its control socket (start it with COMETDESK_CONTROL=1).

  cometdeskctl.py status
  cometdeskctl.py connect Comet
  cometdeskctl.py screenshot [path]        # the app window (video + UI) as PNG
  cometdeskctl.py snapshot [path]          # the device's JPEG of the target
  cometdeskctl.py ocr                      # text on the target screen (macOS)
  cometdeskctl.py type 'hello'             # key presses on the target
  cometdeskctl.py key Enter | combo ControlLeft AltLeft Delete
  cometdeskctl.py click 0.5 0.5 [left|right]   # target position in 0..1
  cometdeskctl.py launch [device]      # start the app (COMETDESK_CONTROL set), wait for the socket, connect
  cometdeskctl.py wait connected|video|idle [secs]   # poll status; exit 1 on timeout
  cometdeskctl.py raw '{"cmd":"wheel","dy":-3}'
Exit status is 1 when the app answers ok=false.
Also usable as a module: `from cometdeskctl import Client; Client().call("status")`.
"""
import json, os, socket, sys, time


def default_socket():
    p = os.environ.get("COMETDESK_CONTROL", "1")
    if p not in ("1", ""):
        return p
    home = os.path.expanduser("~")
    for c in (f"{home}/Library/Application Support/cometdesk/control.sock",
              f"{os.environ.get('XDG_CONFIG_HOME', home + '/.config')}/cometdesk/control.sock"):
        if os.path.exists(c):
            return c
    sys.exit("no control socket; start the app with COMETDESK_CONTROL=1")


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


CONDITIONS = {
    "connected": lambda st: st.get("screen") == "console"
    and st["console"].get("connection") == "Connected",
    "video": lambda st: st.get("screen") == "console" and st.get("video") is not None,
    "idle": lambda st: st.get("screen") == "devices",
}


def launch(device=None, timeout=30):
    """Starts the GUI (debug build unless COMETDESK_BIN) and returns a Client; connects to `device`."""
    import subprocess
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    path = os.environ.get("COMETDESK_CONTROL", "")
    path = path if path not in ("", "1") else "/tmp/cometdesk-control.sock"
    if os.path.exists(path):
        os.unlink(path)
    env = {**os.environ, "COMETDESK_CONTROL": path}
    log = open(os.environ.get("COMETDESK_LOG", "/tmp/cometdesk-gui.log"), "w")
    subprocess.Popen([os.environ.get("COMETDESK_BIN", f"{root}/target/debug/cometdesk-gui")],
                     env=env, stdout=log, stderr=log, start_new_session=True)
    end = time.time() + timeout
    while time.time() < end:
        try:
            c = Client(path)
            break
        except OSError:
            time.sleep(0.2)
    else:
        raise TimeoutError("control socket did not appear; see COMETDESK_LOG")
    if device:
        r = c.call("connect", name=device)
        if not r.get("ok"):
            raise RuntimeError(r)
        c.wait(CONDITIONS["connected"], timeout)
    return c


def main(a):
    if not a:
        sys.exit(__doc__)
    c, rest = a[0], a[1:]
    one = lambda k: {k: rest[0]} if rest else {}
    if c == "launch":
        cl = launch(rest[0] if rest else None)
        print(json.dumps(cl.call("status")))
        return
    if c == "wait":
        try:
            Client().wait(CONDITIONS[rest[0]], float(rest[1]) if len(rest) > 1 else 30)
        except (TimeoutError, KeyError):
            sys.exit(1)
        return
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
