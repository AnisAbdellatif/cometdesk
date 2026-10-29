#!/usr/bin/env python3
"""On-device test of the relay's session rules. Run on the Comet while cometdesk-bridge runs:

    ssh root@<comet> python3 - < bridge/selftest.py

Uses the client unix socket directly (no nginx/auth) and the agent TCP port.
"""

import asyncio

import aiohttp

SOCK = "/run/kvmd/cometdesk-bridge.sock"
AGENT = ("192.168.5.1", 47474)


async def client(session):
    return await session.ws_connect("http://x/ws")


async def recv_client(ws, timeout=2.0):
    msg = await asyncio.wait_for(ws.receive(), timeout)
    return msg.data if msg.type == aiohttp.WSMsgType.BINARY else ("CLOSED", ws.close_code)


async def recv_agent(reader, timeout=2.0):
    data = await asyncio.wait_for(reader.read(100), timeout)
    return data if data else "EOF"


async def main():
    async with aiohttp.ClientSession(connector=aiohttp.UnixConnector(path=SOCK)) as s:
        # 1. Client first: its bytes wait for the agent.
        c = await client(s)
        await c.send_bytes(b"hs1")
        await asyncio.sleep(0.2)
        r, w = await asyncio.open_connection(*AGENT)
        assert await recv_agent(r) == b"hs1", "buffered client bytes reach a new agent"
        w.write(b"hs2")
        await w.drain()
        assert await recv_client(c) == b"hs2", "agent -> client"
        print("ok 1: client-first pairing with buffered handshake")

        # 2. Agent drops: the paired client is closed too.
        w.close()
        got = await recv_client(c)
        assert got[0] == "CLOSED" and got[1] == 4001, got
        print("ok 2: agent drop closes the paired client (4001)")

        # 3. Agent first, then client; then the client drops: agent gets EOF.
        r, w = await asyncio.open_connection(*AGENT)
        await asyncio.sleep(0.2)
        c = await client(s)
        await c.send_bytes(b"x")
        assert await recv_agent(r) == b"x"
        await c.close()
        assert await recv_agent(r) == "EOF", "client drop closes the paired agent"
        w.close()
        print("ok 3: agent-first pairing; client drop closes the agent")

        # 4. A newer client replaces a paired one: old client and agent are both reset,
        #    and nothing from the old session leaks into the new one.
        r, w = await asyncio.open_connection(*AGENT)
        c1 = await client(s)
        await c1.send_bytes(b"old")
        assert await recv_agent(r) == b"old"
        c2 = await client(s)
        got = await recv_client(c1)
        assert got[0] == "CLOSED", got
        assert await recv_agent(r) == "EOF"
        await c2.send_bytes(b"new")
        r2, w2 = await asyncio.open_connection(*AGENT)
        assert await recv_agent(r2) == b"new", "new agent gets only the new session's bytes"
        w2.close()
        await c2.close()
        print("ok 4: newer client resets the session; no stale bytes")

        async with s.get("http://x/status") as resp:
            st = await resp.json()
        print("status:", st["version"], "agent sessions", st["agent"]["sessions"])


asyncio.run(main())
