import socket
import threading
import time
import unittest
from collections.abc import Callable

from udp_probe import measure_udp_loss


class UdpProbeTests(unittest.TestCase):
    def run_probe(
        self, respond: Callable[[socket.socket, bytes, tuple[str, int]], None]
    ) -> tuple[int, int]:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as server:
            server.bind(("127.0.0.1", 0))
            server.settimeout(0.05)
            stop = threading.Event()

            def serve() -> None:
                while not stop.is_set():
                    try:
                        data, address = server.recvfrom(65535)
                    except socket.timeout:
                        continue
                    respond(server, data, address)

            thread = threading.Thread(target=serve)
            thread.start()
            try:
                return measure_udp_loss(
                    "127.0.0.1",
                    server.getsockname()[1],
                    30,
                    interval=0.002,
                    response_timeout=0.1,
                )
            finally:
                stop.set()
                thread.join()

    def test_receives_all_echoes(self) -> None:
        def respond(server: socket.socket, data: bytes, addr: tuple[str, int]) -> None:
            server.sendto(data, addr)

        result = self.run_probe(respond)
        self.assertEqual(result, (30, 30))

    def test_counts_unique_echoes_with_loss_reordering_and_duplicates(self) -> None:
        pending: list[bytes] = []

        def respond(server: socket.socket, data: bytes, addr: tuple[str, int]) -> None:
            sequence = int(data.removeprefix(b"pkt-"))
            if sequence % 5 == 0:
                return
            pending.append(data)
            if len(pending) == 2:
                for payload in reversed(pending):
                    server.sendto(payload, addr)
                    server.sendto(payload, addr)
                server.sendto(b"unrelated", addr)
                pending.clear()

        self.assertEqual(self.run_probe(respond), (30, 24))

    def test_total_loss_uses_one_response_timeout(self) -> None:
        started = time.monotonic()
        self.assertEqual(self.run_probe(lambda _server, _data, _addr: None), (30, 0))
        # Thirty sequential response timeouts alone would take three seconds.
        self.assertLess(time.monotonic() - started, 1.5)


if __name__ == "__main__":
    unittest.main()
