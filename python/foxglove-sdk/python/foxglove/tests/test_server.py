import socket
import ssl
import time
import typing
from pathlib import Path
from urllib.parse import parse_qs, urlparse

import pytest
from foxglove import (
    Channel,
    Context,
    ServerListener,
    Service,
    ServiceSchema,
    StatusLevel,
    start_server,
)
from foxglove.websocket import Capability, PlaybackState, PlaybackStatus


def test_server_interface() -> None:
    """
    Exercise the server interface; will also be checked with mypy.
    """
    server = start_server(
        port=0, session_id="test-session", channel_filter=lambda _: True
    )
    assert isinstance(server.port, int)
    assert server.port != 0

    raw_url = server.app_url()
    assert raw_url is not None
    url = urlparse(raw_url)
    assert url.scheme == "https"
    assert url.netloc == "app.foxglove.dev"
    assert parse_qs(url.query) == {
        "ds": ["foxglove-websocket"],
        "ds.url": [f"ws://127.0.0.1:{server.port}"],
    }

    raw_url = server.app_url(layout_id="lay_123", open_in_desktop=True)
    assert raw_url is not None
    url = urlparse(raw_url)
    assert url.scheme == "https"
    assert url.netloc == "app.foxglove.dev"
    assert parse_qs(url.query) == {
        "ds": ["foxglove-websocket"],
        "ds.url": [f"ws://127.0.0.1:{server.port}"],
        "layoutId": ["lay_123"],
        "openIn": ["desktop"],
    }

    server.publish_status("test message", StatusLevel.Info, "some-id")
    server.broadcast_time(time.time_ns())
    server.broadcast_playback_state(
        PlaybackState(
            status=PlaybackStatus.Paused,
            playback_speed=1.0,
            current_time=time.time_ns(),
            did_seek=False,
            request_id=None,
        )
    )
    server.remove_status(["some-id"])
    server.clear_session("new-session")
    server.stop()


def test_status_level_enum() -> None:
    """Hand-written `#[pyclass]` enums expose `.name` and `.value`."""
    assert StatusLevel.Info.name == "Info"
    assert StatusLevel.Info.value == 0


def test_server_listener_provides_default_implementation() -> None:
    class DefaultServerListener(ServerListener):
        pass

    listener = DefaultServerListener()

    listener.on_parameters_subscribe(["test"])
    listener.on_parameters_unsubscribe(["test"])


def test_services_interface() -> None:
    test_svc = Service(
        name="test",
        schema=ServiceSchema(name="test-schema"),
        handler=lambda *_: b"{}",
    )
    test2_svc = Service(
        name="test2",
        schema=ServiceSchema(name="test-schema"),
        handler=lambda *_: b"{}",
    )
    server = start_server(
        port=0,
        capabilities=[Capability.Services],
        supported_encodings=["json"],
        services=[test_svc],
    )

    # Add a new service.
    server.add_services([test2_svc])

    # Can't add a service with the same name.
    with pytest.raises(RuntimeError):
        server.add_services([test_svc])

    # Remove services.
    server.remove_services(["test", "test2"])

    # Re-add a service.
    server.add_services([test_svc])

    server.stop()


def test_context_can_be_attached_to_server() -> None:
    ctx1 = Context()
    ctx2 = Context()

    server1 = start_server(port=0, context=ctx1)
    server2 = start_server(port=0, context=ctx2)

    ch1 = Channel("/1", context=ctx1)
    ch2 = Channel("/2", context=ctx2)
    ch1.log("test")
    ch2.log("test")

    server1.stop()
    server2.stop()


@typing.no_type_check
def test_server_with_invalid_playback_time_range() -> None:
    with pytest.raises(TypeError):
        # Tuple of a single element
        start_server(port=0, playback_time_range=(123,))

    with pytest.raises(TypeError):
        # Tuple with invalid types
        start_server(port=0, playback_time_range=("not-a-time", None))

    with pytest.raises(TypeError):
        # Not a tuple
        start_server(port=0, playback_time_range=23443)

    with pytest.raises(TypeError):
        # Tuple with too many elements
        start_server(port=0, playback_time_range=(123, 456, 789))


def _tls_fixture(name: str) -> bytes:
    return (Path(__file__).parent / "fixtures" / name).read_bytes()


def test_server_with_tls_accepts_secure_connections() -> None:
    cert = _tls_fixture("tls_cert.pem")
    key = _tls_fixture("tls_key.pem")
    server = start_server(port=0, tls_cert=cert, tls_key=key)
    try:
        raw_url = server.app_url()
        assert raw_url is not None
        assert parse_qs(urlparse(raw_url).query)["ds.url"] == [
            f"wss://127.0.0.1:{server.port}"
        ]

        # Perform a TLS handshake against the server, trusting the self-signed
        # certificate, then complete a WebSocket upgrade over the secure stream.
        ssl_context = ssl.create_default_context(cadata=cert.decode())
        with socket.create_connection(("127.0.0.1", server.port), timeout=5) as sock:
            with ssl_context.wrap_socket(sock, server_hostname="127.0.0.1") as tls:
                assert tls.version() is not None
                tls.sendall(
                    (
                        f"GET / HTTP/1.1\r\n"
                        f"Host: 127.0.0.1:{server.port}\r\n"
                        "Upgrade: websocket\r\n"
                        "Connection: Upgrade\r\n"
                        "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n"
                        "Sec-WebSocket-Version: 13\r\n"
                        "Sec-WebSocket-Protocol: foxglove.sdk.v1\r\n"
                        "\r\n"
                    ).encode()
                )
                response = tls.recv(4096)
                assert response.startswith(b"HTTP/1.1 101")
    finally:
        server.stop()


def test_server_with_tls_rejects_plaintext_connections() -> None:
    server = start_server(
        port=0,
        tls_cert=_tls_fixture("tls_cert.pem"),
        tls_key=_tls_fixture("tls_key.pem"),
    )
    try:
        with socket.create_connection(("127.0.0.1", server.port), timeout=5) as sock:
            sock.sendall(
                (
                    f"GET / HTTP/1.1\r\nHost: 127.0.0.1:{server.port}\r\n"
                    "Upgrade: websocket\r\nConnection: Upgrade\r\n"
                    "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n"
                    "Sec-WebSocket-Version: 13\r\n\r\n"
                ).encode()
            )
            # A plaintext request is not a TLS ClientHello; the server closes the connection
            # without completing a WebSocket upgrade.
            response = sock.recv(4096)
            assert not response.startswith(b"HTTP/1.1 101")
    finally:
        server.stop()


def test_server_with_invalid_tls_config() -> None:
    cert = _tls_fixture("tls_cert.pem")
    key = _tls_fixture("tls_key.pem")

    with pytest.raises(ValueError):
        start_server(port=0, tls_cert=cert)

    with pytest.raises(ValueError):
        start_server(port=0, tls_key=key)

    with pytest.raises(ValueError):
        start_server(port=0, tls_cert=b"", tls_key=key)

    with pytest.raises(RuntimeError):
        start_server(port=0, tls_cert=b"not a certificate", tls_key=key)

    with pytest.raises(RuntimeError):
        start_server(port=0, tls_cert=cert, tls_key=b"not a key")
