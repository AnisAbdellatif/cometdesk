#!/usr/bin/env python3
"""cometdesk-bridge (prototype): relay between a target agent and a remote client.

Runs on the Comet. The target agent connects over the USB network link
(TCP 192.168.5.1:47474). The client (cometdesk-gui) connects through kvmd's nginx at
wss://<comet>/cometdesk-bridge/ws, which applies the normal kvmd login check. The relay
pairs the two and forwards bytes both ways without looking at them; framing and
end-to-end encryption are the agent's and client's business (docs/clipboard-bridge.md).

One agent and one client at a time; a new connection on either side replaces the old
one. Each pairing is one session: when either side connects or goes away, a counterpart
that was already paired with the previous connection is closed too (it reconnects), and
bytes from the old session are discarded. That way both ends always start their
handshake on a clean stream. Bytes sent by a side that is still waiting for its first
partner are buffered up to --max-buffer and delivered when the partner arrives.

It also serves the target's installer scripts and agent binaries on the USB link
(http://192.168.5.1:47475, unreachable from the LAN), and reports its health (firmware,
USB link, stored agents) at /cometdesk-bridge/status for the client's setup/repair check.
Agent binaries are uploaded by the client: PUT /cometdesk-bridge/agents/<name>.
"""

import argparse
import asyncio
import glob
import hashlib
import logging
import os
import pwd
import re
import socket
import tempfile
import time

from aiohttp import WSMsgType, web

VERSION = "0.3.1"
log = logging.getLogger("cometdesk-bridge")

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
AGENTS_DIR = os.path.join(BASE_DIR, "agents")
TARGET_DIR = os.path.join(BASE_DIR, "target")
AGENT_NAME = re.compile(r"[a-z0-9][a-z0-9._-]{0,63}")
MAX_AGENT_SIZE = 32 * 1024 * 1024


class Side:
    """One connected side: who it is, whether it has been paired, bytes waiting for it."""

    def __init__(self, name: str, max_buffer: int) -> None:
        self.name = name
        self.max_buffer = max_buffer
        self.conn = None  # asyncio.StreamWriter (agent) or WebSocketResponse (client)
        self.peered = False
        self.pending = bytearray()  # bytes for this side, sent while it was absent
        self.dropped = 0
        self.since: float | None = None
        self.peer = ""
        self.sessions = 0

    def buffer(self, data: bytes) -> None:
        if len(self.pending) + len(data) > self.max_buffer:
            self.dropped += len(self.pending) + len(data)
            self.pending.clear()
            log.warning("%s absent, buffer full: dropped %d bytes so far", self.name, self.dropped)
            return
        self.pending += data


class Bridge:
    def __init__(self, max_buffer: int) -> None:
        self.agent = Side("agent", max_buffer)
        self.client = Side("client", max_buffer)
        self.bytes_up = 0    # agent -> client
        self.bytes_down = 0  # client -> agent

    # ----- session bookkeeping

    def _close(self, side: Side, reason: str) -> None:
        """Closes `side`'s connection without triggering its own disconnect handling."""
        conn, side.conn = side.conn, None
        side.peered, side.since = False, None
        side.pending.clear()
        if conn is None:
            return
        log.info("closing %s: %s", side.name, reason)
        if side is self.agent:
            conn.close()
        else:
            asyncio.ensure_future(conn.close(code=4001, message=reason.encode()))

    def _attach(self, side: Side, other: Side, conn, peer: str) -> None:
        if side.conn is not None:
            self._close(side, "replaced by a newer connection")
        if other.peered:
            # `other` belongs to the previous session: reset it, and drop what it sent.
            self._close(other, "peer reconnected")
            side.pending.clear()
        # Otherwise `other` is waiting for its first partner and side.pending holds its
        # opening bytes (e.g. the client's handshake): keep them for this connection.
        side.conn, side.peered, side.peer, side.since = conn, False, peer, time.time()
        log.info("%s connected: %s", side.name, peer)

    def _detach(self, side: Side, other: Side, conn) -> None:
        if side.conn is not conn:
            return  # already replaced or closed by the bridge
        side.conn, side.peered, side.since = None, False, None
        side.pending.clear()
        log.info("%s disconnected: %s", side.name, side.peer)
        if other.peered:
            self._close(other, "peer disconnected")

    def _pair_if_both(self) -> list[tuple[Side, bytes]]:
        """When both sides are present and not yet paired, pair them. Returns buffered bytes
        to flush as (destination, data)."""
        if self.agent.conn is None or self.client.conn is None or self.agent.peered:
            return []
        self.agent.peered = self.client.peered = True
        self.agent.sessions += 1
        self.client.sessions += 1
        log.info("paired agent %s with client %s", self.agent.peer, self.client.peer)
        flush = []
        for s in (self.agent, self.client):
            if s.pending:
                flush.append((s, bytes(s.pending)))
                s.pending.clear()
        return flush

    async def _deliver(self, dest: Side, data: bytes) -> None:
        if dest.conn is None:
            dest.buffer(data)
        elif dest is self.agent:
            dest.conn.write(data)
            await dest.conn.drain()
        else:
            await dest.conn.send_bytes(data)

    # ----- agent side (TCP over usb0)
    async def handle_agent(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        sock = writer.get_extra_info("socket")
        if sock is not None:
            sock.setsockopt(socket.SOL_SOCKET, socket.SO_KEEPALIVE, 1)
        peer = "%s:%s" % writer.get_extra_info("peername")[:2]
        self._attach(self.agent, self.client, writer, peer)
        try:
            for dest, data in self._pair_if_both():
                await self._deliver(dest, data)
            while data := await reader.read(65536):
                if self.agent.conn is not writer:
                    break  # replaced or reset meanwhile
                self.bytes_up += len(data)
                await self._deliver(self.client, data)
        except (ConnectionError, asyncio.IncompleteReadError):
            pass
        finally:
            self._detach(self.agent, self.client, writer)
            writer.close()

    # ----- client side (WebSocket via nginx)
    async def handle_client(self, request: web.Request) -> web.WebSocketResponse:
        ws = web.WebSocketResponse(heartbeat=30.0, max_msg_size=8 * 1024 * 1024)
        await ws.prepare(request)
        peer = request.headers.get("X-Real-IP") or request.headers.get("X-Forwarded-For") or "?"
        self._attach(self.client, self.agent, ws, peer)
        try:
            for dest, data in self._pair_if_both():
                await self._deliver(dest, data)
            async for msg in ws:
                if self.client.conn is not ws:
                    break
                if msg.type == WSMsgType.BINARY:
                    data = msg.data
                elif msg.type == WSMsgType.TEXT:
                    data = msg.data.encode()
                else:
                    break
                self.bytes_down += len(data)
                await self._deliver(self.agent, data)
        except ConnectionError:
            pass
        finally:
            self._detach(self.client, self.agent, ws)
        return ws

    async def handle_status(self, _: web.Request) -> web.Response:
        def side(s: Side) -> dict:
            connected = s.conn is not None
            return {
                "connected": connected,
                "paired": s.peered,
                "peer": s.peer if connected else None,
                "since": s.since,
                "sessions": s.sessions,
                "pending_bytes": len(s.pending),
                "dropped_bytes": s.dropped,
            }

        return web.json_response({
            "version": VERSION,
            "link": "rndis",
            "firmware": firmware_version(),
            "bundle": _read(os.path.join(BASE_DIR, "bundle")) or None,
            "usb": usb_health(),
            "agents": list_agents(),
            "agent": side(self.agent),
            "client": side(self.client),
            "bytes_agent_to_client": self.bytes_up,
            "bytes_client_to_agent": self.bytes_down,
        })


# ----- health

def _read(path: str) -> str:
    try:
        with open(path) as f:
            return f.read().strip()
    except OSError:
        return ""


def firmware_version() -> str:
    for line in _read("/etc/version").splitlines():
        if line.startswith("RK_VERSION="):
            return line.split("=", 1)[1]
    return ""


def usb_health() -> dict:
    """Whether the RNDIS function is in the USB gadget, usb0 exists, and a host is attached."""
    return {
        "gadget": bool(glob.glob("/sys/kernel/config/usb_gadget/*/configs/*/rndis.usb0")),
        "iface": os.path.isdir("/sys/class/net/usb0"),
        "carrier": _read("/sys/class/net/usb0/carrier") == "1",
    }


# ----- agent binaries (uploaded by the client, downloaded by the target)

def _agent_path(name: str) -> str:
    if not AGENT_NAME.fullmatch(name):
        raise web.HTTPBadRequest(text="bad agent name")
    return os.path.join(AGENTS_DIR, name)


_digests: dict[str, tuple[float, int, str]] = {}  # name -> (mtime, size, sha256)


def list_agents() -> list[dict]:
    out = []
    for name in sorted(os.listdir(AGENTS_DIR)) if os.path.isdir(AGENTS_DIR) else []:
        path = os.path.join(AGENTS_DIR, name)
        if not AGENT_NAME.fullmatch(name) or not os.path.isfile(path):
            continue
        st = os.stat(path)
        cached = _digests.get(name)
        if cached is None or cached[:2] != (st.st_mtime, st.st_size):
            with open(path, "rb") as f:
                cached = (st.st_mtime, st.st_size, hashlib.sha256(f.read()).hexdigest())
            _digests[name] = cached
        out.append({"name": name, "size": st.st_size, "sha256": cached[2]})
    return out


async def handle_agent_upload(request: web.Request) -> web.Response:
    path = _agent_path(request.match_info["name"])
    os.makedirs(AGENTS_DIR, exist_ok=True)
    # Unique temp name so concurrent PUTs don't clobber each other; the "." prefix keeps it
    # out of AGENT_NAME (so list_agents/_agent_path never surface an in-flight upload, even
    # one named "x.part"). Cleaned up on any failure.
    fd, tmp = tempfile.mkstemp(dir=AGENTS_DIR, prefix=".upload-")
    digest = hashlib.sha256()
    size = 0
    try:
        with os.fdopen(fd, "wb") as f:
            async for chunk in request.content.iter_chunked(65536):
                size += len(chunk)
                if size > MAX_AGENT_SIZE:
                    raise web.HTTPRequestEntityTooLarge(MAX_AGENT_SIZE, size)
                digest.update(chunk)
                f.write(chunk)
        os.chmod(tmp, 0o644)
        os.replace(tmp, path)
    except BaseException:
        if os.path.exists(tmp):
            os.unlink(tmp)
        raise
    log.info("agent binary %s stored (%d bytes)", os.path.basename(path), size)
    return web.json_response({"name": os.path.basename(path), "size": size, "sha256": digest.hexdigest()})


async def handle_agent_download(request: web.Request) -> web.StreamResponse:
    path = _agent_path(request.match_info["name"])
    if not os.path.isfile(path):
        raise web.HTTPNotFound(text="no such agent binary on the Comet; set up the bridge again")
    return web.FileResponse(path)


def target_script(name: str):
    async def handler(_: web.Request) -> web.StreamResponse:
        return web.FileResponse(os.path.join(TARGET_DIR, name), headers={"Content-Type": "text/plain"})
    return handler


async def serve_agent(bridge: Bridge, host: str, port: int) -> None:
    # usb0 may not have its address yet at boot: keep trying.
    while True:
        try:
            server = await asyncio.start_server(bridge.handle_agent, host, port)
            log.info("agent listener on %s:%d", host, port)
            async with server:
                await server.serve_forever()
        except OSError as ex:
            log.warning("cannot listen on %s:%d (%s), retrying in 5 s", host, port, ex)
            await asyncio.sleep(5)


async def serve_downloads(host: str, port: int) -> None:
    """Installer scripts and agent binaries for the target, on the USB link only."""
    app = web.Application()
    app.router.add_get("/install.sh", target_script("install.sh"))
    app.router.add_get("/install.ps1", target_script("install.ps1"))
    app.router.add_get("/agents/{name}", handle_agent_download)
    runner = web.AppRunner(app, access_log=None)
    await runner.setup()
    while True:
        try:
            await web.TCPSite(runner, host, port).start()
            log.info("target downloads on http://%s:%d", host, port)
            return
        except OSError as ex:
            log.warning("cannot listen on %s:%d (%s), retrying in 5 s", host, port, ex)
            await asyncio.sleep(5)


def _restrict_socket(path: str) -> None:
    """Lock the relay socket down to nginx only.

    nginx proxies to this socket *after* the kvmd login check, so any local uid able to
    connect to it would bypass that login entirely. 0o666 let everyone in; use 0o660 and
    give it to the user nginx runs as. We learn that uid/gid from the owner of nginx's
    pidfile, falling back to the kvmd-nginx / nginx accounts. If none resolve, keep 0o660
    but hand the socket to its directory's group (e.g. /run/kvmd's) so it is at least not
    world-reachable, and log loudly so the operator can verify nginx can still connect.
    """
    os.chmod(path, 0o660)
    try:
        st = os.stat("/run/kvmd/nginx.pid")
        uid, gid, how = st.st_uid, st.st_gid, "nginx.pid owner"
    except OSError:
        uid = gid = -1
        how = ""
        for name in ("kvmd-nginx", "nginx"):
            try:
                ent = pwd.getpwnam(name)
                uid, gid, how = ent.pw_uid, ent.pw_gid, "user " + name
                break
            except KeyError:
                continue
    if how:
        try:
            os.chown(path, uid, gid)
            log.info("relay socket %s: mode 0o660, owned by nginx (%s)", path, how)
            return
        except OSError as ex:
            log.warning("cannot chown relay socket to nginx (%s): %s", how, ex)
    try:
        dir_gid = os.stat(os.path.dirname(path) or ".").st_gid
        os.chown(path, -1, dir_gid)
    except OSError as ex:
        log.warning("cannot set relay socket group: %s", ex)
    log.warning("relay socket %s: mode 0o660 but nginx uid/gid unknown; "
                "confirm nginx can still reach it", path)


async def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--unix", default="/run/kvmd/cometdesk-bridge.sock")
    parser.add_argument("--agent-host", default="192.168.5.1")
    parser.add_argument("--agent-port", type=int, default=47474)
    parser.add_argument("--download-port", type=int, default=47475)
    parser.add_argument("--max-buffer", type=int, default=256 * 1024)
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")

    bridge = Bridge(args.max_buffer)
    app = web.Application(client_max_size=MAX_AGENT_SIZE)
    app.router.add_get("/ws", bridge.handle_client)
    app.router.add_get("/status", bridge.handle_status)
    app.router.add_put("/agents/{name}", handle_agent_upload)
    runner = web.AppRunner(app, access_log=None)
    await runner.setup()
    if os.path.exists(args.unix):
        os.unlink(args.unix)
    await web.UnixSite(runner, args.unix).start()
    _restrict_socket(args.unix)  # nginx-only: a world-writable socket bypasses kvmd login
    log.info("cometdesk-bridge %s: client socket %s", VERSION, args.unix)
    asyncio.ensure_future(serve_downloads(args.agent_host, args.download_port))
    await serve_agent(bridge, args.agent_host, args.agent_port)


if __name__ == "__main__":
    asyncio.run(main())
