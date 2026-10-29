#!/usr/bin/env python3
"""Minimal client-side test for glkvm-bridge (stdlib only).

Connects to wss://<comet>/glkvm-bridge/ws with a kvmd session cookie, sends one
message, and prints whatever the agent side sends back within --wait seconds.

    python3 bridge/test_client.py --host 192.168.8.174 --cookie-file <netscape cookie jar>
"""

import argparse
import base64
import os
import socket
import ssl
import struct
import sys
import time


def auth_token(cookie_file: str) -> str:
    for line in open(cookie_file):
        parts = line.rstrip("\n").split("\t")
        if len(parts) == 7 and parts[5] == "auth_token":
            return parts[6]
    sys.exit("no auth_token in " + cookie_file)


def send_frame(sock: ssl.SSLSocket, payload: bytes, opcode: int = 0x2) -> None:
    mask = os.urandom(4)
    head = bytes([0x80 | opcode])
    n = len(payload)
    if n < 126:
        head += bytes([0x80 | n])
    elif n < 65536:
        head += bytes([0x80 | 126]) + struct.pack(">H", n)
    else:
        head += bytes([0x80 | 127]) + struct.pack(">Q", n)
    sock.sendall(head + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(payload)))


def recv_exact(sock: ssl.SSLSocket, n: int) -> bytes:
    buf = b""
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise ConnectionError("closed")
        buf += chunk
    return buf


def recv_frame(sock: ssl.SSLSocket) -> tuple[int, bytes]:
    b0, b1 = recv_exact(sock, 2)
    n = b1 & 0x7F
    if n == 126:
        n = struct.unpack(">H", recv_exact(sock, 2))[0]
    elif n == 127:
        n = struct.unpack(">Q", recv_exact(sock, 8))[0]
    return b0 & 0x0F, recv_exact(sock, n)


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--host", default="192.168.8.174")
    p.add_argument("--cookie-file", help="curl/Netscape cookie jar with auth_token")
    p.add_argument("--no-auth", action="store_true", help="connect without a token (expect 401)")
    p.add_argument("--message", default="hello-from-mac")
    p.add_argument("--wait", type=float, default=60.0)
    args = p.parse_args()

    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    # DO NOT COPY INTO PRODUCTION CODE. This disables TLS cert verification and is only
    # acceptable here because test_client.py is a throwaway manual test tool pointed at a
    # Comet's self-signed cert. Any real client must verify the cert (or pin it).
    ctx.verify_mode = ssl.CERT_NONE  # prototype/manual-test only; see warning above
    sock = ctx.wrap_socket(socket.create_connection((args.host, 443), timeout=10))

    cookie = "" if args.no_auth else "Cookie: auth_token=%s\r\n" % auth_token(args.cookie_file)
    key = base64.b64encode(os.urandom(16)).decode()
    sock.sendall((
        "GET /glkvm-bridge/ws HTTP/1.1\r\nHost: %s\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
        "Sec-WebSocket-Key: %s\r\nSec-WebSocket-Version: 13\r\n%s\r\n" % (args.host, key, cookie)
    ).encode())
    head = b""
    while b"\r\n\r\n" not in head:
        head += sock.recv(1)
    status = head.split(b"\r\n")[0].decode()
    print("handshake:", status)
    if " 101 " not in status:
        return

    send_frame(sock, args.message.encode())
    print("sent:", args.message)
    deadline = time.time() + args.wait
    while time.time() < deadline:
        sock.settimeout(max(0.1, deadline - time.time()))
        try:
            opcode, data = recv_frame(sock)
        except (socket.timeout, TimeoutError):
            break
        if opcode == 0x8:
            print("closed by server:", data[2:].decode(errors="replace"))
            return
        if opcode == 0x9:  # ping
            send_frame(sock, data, opcode=0xA)
            continue
        print("received:", data.decode(errors="replace").rstrip())
    send_frame(sock, b"\x03\xe8", opcode=0x8)


if __name__ == "__main__":
    main()
