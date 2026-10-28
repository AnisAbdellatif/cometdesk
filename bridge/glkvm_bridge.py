#!/usr/bin/env python3
"""glkvm-bridge (prototype): relay between a target agent and a remote client.

Runs on the Comet. The target agent connects over the USB network link
(TCP 192.168.5.1:47474). The client (glkvm-gui) connects through kvmd's nginx at
wss://<comet>/glkvm-bridge/ws, which applies the normal kvmd login check. The relay
pairs the two and forwards bytes both ways without looking at them; framing and
end-to-end encryption are the agent's and client's business (docs/clipboard-bridge.md).

One agent and one client at a time; a new connection on either side replaces the old
one. Bytes that arrive while the other side is absent are buffered up to --max-buffer
and dropped beyond that.
"""

import argparse
import asyncio
import logging
import os
import time

from aiohttp import WSMsgType, web

VERSION = "0.1.0"
log = logging.getLogger("glkvm-bridge")


class Side:
    """Bytes waiting for one side, plus how to deliver them."""

    def __init__(self, name: str, max_buffer: int) -> None:
        self.name = name
        self.max_buffer = max_buffer
        self.pending = bytearray()
        self.dropped = 0
        self.since: float | None = None
        self.peer = ""

    def buffer(self, data: bytes) -> None:
        if len(self.pending) + len(data) > self.max_buffer:
            self.dropped += len(self.pending) + len(data)
            self.pending.clear()
            log.warning("%s absent, buffer full: dropped %d bytes so far", self.name, self.dropped)
            return
        self.pending += data


class Bridge:
    def __init__(self, max_buffer: int) -> None:
        self.agent_writer: asyncio.StreamWriter | None = None
        self.client_ws: web.WebSocketResponse | None = None
        self.to_agent = Side("agent", max_buffer)
        self.to_client = Side("client", max_buffer)
        self.bytes_up = 0    # agent -> client
        self.bytes_down = 0  # client -> agent

    # ----- agent side (TCP over usb0)
    async def handle_agent(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
        peer = "%s:%s" % writer.get_extra_info("peername")[:2]
        if self.agent_writer is not None:
            log.info("agent %s replaces %s", peer, self.to_agent.peer)
            self.agent_writer.close()
        self.agent_writer = writer
        self.to_agent.peer, self.to_agent.since = peer, time.time()
        log.info("agent connected: %s", peer)
        if self.to_agent.pending:
            writer.write(bytes(self.to_agent.pending))
            self.to_agent.pending.clear()
            await writer.drain()
        try:
            while data := await reader.read(65536):
                self.bytes_up += len(data)
                if self.client_ws is not None and not self.client_ws.closed:
                    await self.client_ws.send_bytes(data)
                else:
                    self.to_client.buffer(data)
        except (ConnectionError, asyncio.IncompleteReadError):
            pass
        finally:
            if self.agent_writer is writer:
                self.agent_writer = None
                self.to_agent.since = None
            writer.close()
            log.info("agent disconnected: %s", peer)

    # ----- client side (WebSocket via nginx)
    async def handle_client(self, request: web.Request) -> web.WebSocketResponse:
        ws = web.WebSocketResponse(heartbeat=30.0, max_msg_size=8 * 1024 * 1024)
        await ws.prepare(request)
        peer = request.headers.get("X-Real-IP") or request.headers.get("X-Forwarded-For") or "?"
        if self.client_ws is not None and not self.client_ws.closed:
            log.info("client %s replaces %s", peer, self.to_client.peer)
            await self.client_ws.close(code=4000, message=b"replaced by a newer client")
        self.client_ws = ws
        self.to_client.peer, self.to_client.since = peer, time.time()
        log.info("client connected: %s", peer)
        if self.to_client.pending:
            await ws.send_bytes(bytes(self.to_client.pending))
            self.to_client.pending.clear()
        try:
            async for msg in ws:
                if msg.type == WSMsgType.BINARY:
                    data = msg.data
                elif msg.type == WSMsgType.TEXT:
                    data = msg.data.encode()
                else:
                    break
                self.bytes_down += len(data)
                if self.agent_writer is not None:
                    self.agent_writer.write(data)
                    await self.agent_writer.drain()
                else:
                    self.to_agent.buffer(data)
        finally:
            if self.client_ws is ws:
                self.client_ws = None
                self.to_client.since = None
            log.info("client disconnected: %s", peer)
        return ws

    async def handle_status(self, _: web.Request) -> web.Response:
        def side(s: Side, connected: bool) -> dict:
            return {
                "connected": connected,
                "peer": s.peer if connected else None,
                "since": s.since,
                "pending_bytes": len(s.pending),
                "dropped_bytes": s.dropped,
            }

        return web.json_response({
            "version": VERSION,
            "link": "rndis",
            "agent": side(self.to_agent, self.agent_writer is not None),
            "client": side(self.to_client, self.client_ws is not None and not self.client_ws.closed),
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
