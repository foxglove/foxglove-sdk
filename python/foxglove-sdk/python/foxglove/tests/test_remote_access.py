import pytest

try:
    from foxglove import start_gateway
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
    assert Capability.Services is not None
    assert Capability.ClientPublish != Capability.Services


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
