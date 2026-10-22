import base64
import json
import os
import socket
import struct
import time
import typing
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
from foxglove.websocket import (
    Capability,
    ChannelView,
    Client,
    PlaybackState,
    PlaybackStatus,
)


def _recv_exact(sock: socket.socket, size: int) -> bytes:
    buf = b""
    while len(buf) < size:
        chunk = sock.recv(size - len(buf))
        if not chunk:
            raise ConnectionError("socket closed")
        buf += chunk
    return buf


def _ws_connect(port: int) -> socket.socket:
    """Opens a minimal WebSocket client connection using the Foxglove subprotocol."""
    sock = socket.create_connection(("127.0.0.1", port), timeout=5)
    key = base64.b64encode(os.urandom(16)).decode()
    sock.sendall(
        (
            "GET / HTTP/1.1\r\n"
            f"Host: 127.0.0.1:{port}\r\n"
            "Upgrade: websocket\r\n"
            "Connection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\n"
            "Sec-WebSocket-Version: 13\r\n"
            "Sec-WebSocket-Protocol: foxglove.sdk.v1\r\n"
            "\r\n"
        ).encode()
    )
    response = b""
    # Read byte-by-byte so no frame data following the handshake is consumed.
    while not response.endswith(b"\r\n\r\n"):
        response += _recv_exact(sock, 1)
    assert response.startswith(b"HTTP/1.1 101")
    return sock


def _ws_recv_json(sock: socket.socket) -> typing.Any:
    """Returns the next text frame from the server as JSON, skipping other frames."""
    while True:
        b0, b1 = _recv_exact(sock, 2)
        size = b1 & 0x7F
        if size == 126:
            (size,) = struct.unpack("!H", _recv_exact(sock, 2))
        elif size == 127:
            (size,) = struct.unpack("!Q", _recv_exact(sock, 8))
        payload = _recv_exact(sock, size)
        if b0 & 0x0F == 0x1:
            return json.loads(payload)


def _ws_send_json(sock: socket.socket, message: typing.Any) -> None:
    payload = json.dumps(message).encode()
    assert len(payload) < 126
    mask = os.urandom(4)
    masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
    sock.sendall(bytes([0x81, 0x80 | len(payload)]) + mask + masked)


def _ws_recv_op(sock: socket.socket, op: str) -> typing.Any:
    while True:
        message = _ws_recv_json(sock)
        if message["op"] == op:
            return message


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


def test_send_status_targets_single_client() -> None:
    """
    send_status, called from a listener callback, reaches only the given client. Unknown
    clients and a stopped server are ignored.
    """
    ctx = Context()
    channel = Channel("/test", context=ctx)

    class Listener(ServerListener):
        def on_subscribe(self, client: Client, channel: ChannelView) -> None:
            server.send_status(client.id, "rejected", StatusLevel.Warning, "status-id")

    server = start_server(port=0, context=ctx, server_listener=Listener())
    target = _ws_connect(server.port)
    bystander = _ws_connect(server.port)
    try:
        _ws_send_json(
            target,
            {
                "op": "subscribe",
                "subscriptions": [{"id": 1, "channelId": channel.id()}],
            },
        )
        assert _ws_recv_op(target, "status") == {
            "op": "status",
            "level": 1,
            "message": "rejected",
            "id": "status-id",
        }

        server.send_status(2**32 - 1, "unknown client", StatusLevel.Error)

        # Per-client messages are delivered in order, so the bystander's first status is the
        # broadcast unless the targeted status was misdelivered.
        server.publish_status("broadcast", StatusLevel.Info)
        assert _ws_recv_op(bystander, "status")["message"] == "broadcast"
    finally:
        target.close()
        bystander.close()
        server.stop()

    server.send_status(0, "stopped", StatusLevel.Info)


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
