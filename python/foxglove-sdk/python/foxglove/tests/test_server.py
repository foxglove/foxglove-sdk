import time
from urllib.parse import parse_qs, urlparse

import pytest
from foxglove import (
    Capability,
    Channel,
    Context,
    ServerListener,
    Service,
    start_server,
)
from foxglove.websocket import ServiceSchema, StatusLevel


def test_server_interface() -> None:
    """
    Exercise the server interface; will also be checked with mypy.
    """
    server = start_server(port=0)
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
    server.remove_status(["some-id"])
    server.clear_session()
    server.stop()


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
