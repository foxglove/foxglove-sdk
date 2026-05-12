import pytest

try:
    from foxglove import ConnectionGraph, start_gateway
    from foxglove.remote_access import (
        Capability,
        RemoteAccessConnectionStatus,
        RemoteAccessListener,
    )

    HAS_REMOTE_ACCESS = True
except ImportError:
    HAS_REMOTE_ACCESS = False

pytestmark = pytest.mark.skipif(
    not HAS_REMOTE_ACCESS, reason="remote_access feature not enabled"
)


def test_start_gateway_requires_device_token() -> None:
    """
    Starting a gateway without a device token (and no env var) should raise an error.
    """
    with pytest.raises(RuntimeError, match="No device token provided"):
        start_gateway()


def test_capability_enum() -> None:
    """
    Verify the Capability enum variants are accessible.
    """
    assert Capability.ClientPublish is not None
    assert Capability.ConnectionGraph is not None
    assert Capability.Services is not None
    assert Capability.ClientPublish != Capability.Services
    assert Capability.ConnectionGraph != Capability.ClientPublish
    assert Capability.Services.name == "Services"
    assert Capability.Services.value == 2


def test_connection_status_enum() -> None:
    """
    Verify the RemoteAccessConnectionStatus enum variants are accessible.
    """
    assert RemoteAccessConnectionStatus.Connecting is not None
    assert RemoteAccessConnectionStatus.Connected is not None
    assert RemoteAccessConnectionStatus.ShuttingDown is not None
    assert RemoteAccessConnectionStatus.Shutdown is not None


def test_listener_provides_default_implementation() -> None:
    class DefaultListener(RemoteAccessListener):
        pass

    listener = DefaultListener()

    listener.on_connection_status_changed(RemoteAccessConnectionStatus.Connecting)
    listener.on_subscribe(None, None)  # type: ignore[arg-type]
    listener.on_unsubscribe(None, None)  # type: ignore[arg-type]
    listener.on_client_advertise(None, None)  # type: ignore[arg-type]
    listener.on_client_unadvertise(None, None)  # type: ignore[arg-type]
    listener.on_message_data(None, None, b"test")  # type: ignore[arg-type]
    listener.on_connection_graph_subscribe()
    listener.on_connection_graph_unsubscribe()


def test_connection_graph_repr() -> None:
    """
    Verify that repr returns a non-empty string.
    """
    graph = ConnectionGraph()
    graph.set_published_topic("/topic1", ["pub1"])
    r = repr(graph)
    assert "topic1" in r
    assert "pub1" in r


def test_connection_graph_construction() -> None:
    """
    Verify that ConnectionGraph can be constructed and populated.
    """
    graph = ConnectionGraph()
    graph.set_published_topic("/topic1", ["pub1", "pub2"])
    graph.set_subscribed_topic("/topic2", ["sub1"])
    graph.set_advertised_service("/svc1", ["provider1", "provider2"])
    r = repr(graph)
    assert "topic1" in r
    assert "pub1" in r
    assert "pub2" in r
    assert "topic2" in r
    assert "sub1" in r
    assert "svc1" in r
    assert "provider1" in r
    assert "provider2" in r


def test_connection_graph_overwrite_topic() -> None:
    """
    Verify that setting a topic again overwrites the previous entry.
    """
    graph = ConnectionGraph()
    graph.set_published_topic("/topic1", ["pub1"])
    graph.set_published_topic("/topic1", ["pub2", "pub3"])
    r = repr(graph)
    assert "topic1" in r
    assert "pub2" in r
    assert "pub3" in r
    assert "pub1" not in r


def test_connection_graph_empty_ids() -> None:
    """
    Verify that empty ID lists are accepted.
    """
    graph = ConnectionGraph()
    graph.set_published_topic("/empty-topic", [])
    graph.set_subscribed_topic("/empty-sub", [])
    graph.set_advertised_service("/empty-svc", [])
    r = repr(graph)
    assert "empty-topic" in r
    assert "empty-sub" in r
    assert "empty-svc" in r


def test_connection_graph_capability_in_remote_access() -> None:
    """
    Verify ConnectionGraph capability is importable from remote_access module.
    """
    from foxglove.remote_access import Capability, ConnectionGraph as CG

    assert CG is not None
    assert Capability.ConnectionGraph is not None
