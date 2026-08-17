import os

import pytest

# Set FOXGLOVE_TEST_REQUIRE_REMOTE_ACCESS=1 in environments where remote_access
# is expected to be available (e.g. CI jobs building wheels with the
# remote-access feature). This converts the soft skip below into a hard failure
# so a broken or accidentally-disabled remote_access build doesn't slip through.
_REQUIRE_REMOTE_ACCESS = os.environ.get("FOXGLOVE_TEST_REQUIRE_REMOTE_ACCESS") == "1"

try:
    from foxglove import ConnectionGraph, start_gateway
    from foxglove.remote_access import (
        Capability,
        DracoEncodeOptions,
        RemoteAccessConnectionStatus,
        RemoteAccessListener,
    )

    HAS_REMOTE_ACCESS = True
except ImportError:
    if _REQUIRE_REMOTE_ACCESS:
        raise
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


def test_start_gateway_accepts_asset_handler() -> None:
    """
    The asset handler is accepted as a keyword argument; the gateway then fails on the
    missing token.
    """
    with pytest.raises(RuntimeError, match="No device token provided"):
        start_gateway(asset_handler=lambda _uri: b"asset")


def test_start_gateway_accepts_point_cloud_compression_policy() -> None:
    """
    The point-cloud compression policy is validated before the gateway starts, so the
    missing-token error proves it was accepted.
    """
    with pytest.raises(RuntimeError, match="No device token provided"):
        start_gateway(
            point_cloud_compression=lambda _channel: DracoEncodeOptions(
                quantization_bits=10
            ),
        )

    with pytest.raises(RuntimeError, match="No device token provided"):
        start_gateway(point_cloud_compression=lambda _channel: False)


def test_start_gateway_rejects_non_callable_point_cloud_compression() -> None:
    """
    Earlier revisions accepted ``DracoEncodeOptions | bool`` directly; those values must
    now be rejected at startup with a pointer at the per-channel callable, since e.g.
    ``False`` would otherwise fall back to default compression — the opposite of what the
    caller asked for.
    """
    for value in (False, True, DracoEncodeOptions()):
        with pytest.raises(TypeError, match="must be a callable"):
            start_gateway(point_cloud_compression=value)  # type: ignore[arg-type]


def test_draco_encode_options_defaults() -> None:
    options = DracoEncodeOptions()
    assert options.quantization_bits == 12

    options = DracoEncodeOptions(quantization_bits=10)
    assert options.quantization_bits == 10


def test_draco_encode_options_reject_invalid_quantization_bits() -> None:
    # Out-of-range values are rejected at construction with a ValueError naming the
    # offending value. 0 is lossless, so its message carries the "disable compression"
    # remediation; 31 is the first value above the reference Draco decoder's 30-bit cap,
    # which just wants a smaller number, so the hint must not appear there.
    with pytest.raises(
        ValueError,
        match=r"quantization_bits \(0\).*return False",
    ):
        DracoEncodeOptions(quantization_bits=0)

    with pytest.raises(ValueError, match=r"quantization_bits \(31\)") as excinfo:
        DracoEncodeOptions(quantization_bits=31)
    assert "return False" not in str(excinfo.value)

    # quantization_bits must fit in a u8.
    with pytest.raises(OverflowError):
        DracoEncodeOptions(quantization_bits=256)


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
    assert Capability.Services.value == 3
    assert Capability.ConnectionGraph.value == 1


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
    from foxglove.remote_access import Capability
    from foxglove.remote_access import ConnectionGraph as CG

    assert CG is not None
    assert Capability.ConnectionGraph is not None
