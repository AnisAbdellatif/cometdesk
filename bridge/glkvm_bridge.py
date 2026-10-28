#!/usr/bin/env python3
"""glkvm-bridge (prototype): relay between a target agent and a remote client.

Runs on the Comet. The target agent connects over the USB network link
(TCP 192.168.5.1:47474). The client (glkvm-gui) connects through kvmd's nginx at
wss://<comet>/glkvm-bridge/ws, which applies the normal kvmd login check. The relay
pairs the two and forwards bytes both ways without looking at them; framing and
end-to-end encryption are the agent's and client's business (docs/clipboard-bridge.md).

One agent and one client at a time; a new connection on either side replaces the old
one. Each pairing is one session: when either side connects or goes away, a counterpart
that was already paired with the previous connection is closed too (it reconnects), and
bytes from the old session are discarded. That way both ends always start their
handshake on a clean stream. Bytes sent by a side that is still waiting for its first
partner are buffered up to --max-buffer and delivered when the partner arrives.
"""

import argparse
import asyncio
import logging
import os
import socket
import time

from aiohttp import WSMsgType, web

VERSION = "0.2.0"
log = logging.getLogger("glkvm-bridge")


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
            "agent": side(self.agent),
            "client": side(self.client),
            "bytes_agent_to_client": self.bytes_up,
            "bytes_client_to_agent": self.bytes_down,
        })


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


async def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--unix", default="/run/kvmd/glkvm-bridge.sock")
    parser.add_argument("--agent-host", default="192.168.5.1")
    parser.add_argument("--agent-port", type=int, default=47474)
    parser.add_argument("--max-buffer", type=int, default=256 * 1024)
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")

    bridge = Bridge(args.max_buffer)
    app = web.Application()
    app.router.add_get("/ws", bridge.handle_client)
    app.router.add_get("/status", bridge.handle_status)
    runner = web.AppRunner(app, access_log=None)
    await runner.setup()
    if os.path.exists(args.unix):
        os.unlink(args.unix)
    await web.UnixSite(runner, args.unix).start()
    os.chmod(args.unix, 0o666)  # nginx connects as its own user
    log.info("glkvm-bridge %s: client socket %s", VERSION, args.unix)
    await serve_agent(bridge, args.agent_host, args.agent_port)


if __name__ == "__main__":
    asyncio.run(main())
