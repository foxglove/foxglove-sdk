#!/usr/bin/env python3
"""Stateless UDP echo server for netem packet loss measurement.

Echoes every received datagram back to its sender from a single unconnected
socket. Used by the netem test stacks (`docker-compose.netem.yml` port 9999,
`docker-compose.netem-perlink.yml` port 7001 on the targets).

A single-socket loop is used deliberately instead of a forking server such as
`socat UDP-RECVFROM:<port>,fork`: forked children share the bound socket with
the parent, and a child that lingers (e.g. after receiving the 0-length
datagram a socat client emits at EOF, which netem jitter can reorder ahead of
the payload) steals datagrams from new clients and never replies. That race
intermittently blackholed the per-link loss measurements in CI (FLE-595).

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
        # Skip empty datagrams: socat clients send one at EOF as an
        # end-of-stream marker, and echoing it back would only trigger ICMP
        # errors once the client's ephemeral port is closed.
        if not data:
            continue
        try:
            sock.sendto(data, addr)
        except OSError:
            # A reply can fail (e.g. ICMP error queued by a previous send);
            # the server must keep serving other clients regardless.
            continue


if __name__ == "__main__":
    main()
