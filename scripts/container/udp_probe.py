#!/usr/bin/env python3
"""Measure UDP echo loss using paced probes and a shared response deadline."""

import socket
import sys
import time


def measure_udp_loss(
    target_ip: str,
    port: int,
    count: int,
    interval: float = 0.01,
    response_timeout: float = 1.0,
) -> tuple[int, int]:
    """Count unique echoed probes, allowing delayed and reordered responses.

    Pace sends to avoid overflowing the echo server or netem queues. The response
    deadline starts after the last send so loss costs one timeout per batch.
    """
    sent = 0
    received: set[bytes] = set()
    payloads = [f"pkt-{i}".encode() for i in range(count)]
    expected = set(payloads)
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.connect((target_ip, port))
        next_send = time.monotonic()
        deadline = next_send + response_timeout
        while sent < count or time.monotonic() < deadline:
            now = time.monotonic()
            if sent < count and now >= next_send:
                sock.send(payloads[sent])
                sent += 1
                next_send = time.monotonic() + interval
                deadline = time.monotonic() + response_timeout
            if sent == count and len(received) == count:
                break
            wake_at = next_send if sent < count else deadline
            sock.settimeout(max(wake_at - time.monotonic(), 0.000001))
            try:
                data = sock.recv(65535)
            except socket.timeout:
                continue
            if data in expected:
                received.add(data)
    return sent, len(received)


if __name__ == "__main__":
    sent_count, received_count = measure_udp_loss(
        sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    )
    print(sent_count, received_count)
