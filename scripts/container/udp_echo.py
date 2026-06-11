#!/usr/bin/env python3
"""Stateless UDP echo server for netem packet loss measurement.

Echoes every received datagram back to its sender from a single unconnected
socket. The single-socket loop is deliberate: forking servers share the bound
socket with their children and can swallow datagrams from new clients (FLE-595).

Usage: udp_echo.py <port>
"""

import socket
import sys


def main() -> None:
    port = int(sys.argv[1])
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(("0.0.0.0", port))
    print(f"udp_echo: listening on {port}", flush=True)
    while True:
        data, addr = sock.recvfrom(65535)
        # Ignore empty datagrams: socat clients send one at EOF.
        if not data:
            continue
        try:
            sock.sendto(data, addr)
        except OSError:
            # Keep serving if a reply fails (e.g. a queued ICMP error).
            continue


if __name__ == "__main__":
    main()
