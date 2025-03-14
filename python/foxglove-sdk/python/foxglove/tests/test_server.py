import time

import pytest
from foxglove import (
    Capability,
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
