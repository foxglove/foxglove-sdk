import pytest

try:
    from foxglove import (
        Context,
        SystemInfoPublisher,
        start_sysinfo_publisher,
    )

    HAS_SYSINFO = True
except ImportError:
    HAS_SYSINFO = False

pytestmark = pytest.mark.skipif(
    not HAS_SYSINFO, reason="sysinfo publisher not available on this platform"
)


def test_start_with_defaults() -> None:
    publisher = start_sysinfo_publisher()
    assert isinstance(publisher, SystemInfoPublisher)
    publisher.stop()


def test_start_with_custom_topic_and_interval() -> None:
    publisher = start_sysinfo_publisher(
        topic="/custom/sysinfo",
        refresh_interval=0.5,
    )
    publisher.stop()


def test_start_with_context() -> None:
    ctx = Context()
    publisher = start_sysinfo_publisher(context=ctx, refresh_interval=0.25)
    publisher.stop()


def test_stop_is_idempotent() -> None:
    publisher = start_sysinfo_publisher()
    publisher.stop()
    publisher.stop()


def test_invalid_refresh_interval_raises() -> None:
    with pytest.raises(ValueError):
        start_sysinfo_publisher(refresh_interval=-1.0)

    with pytest.raises(ValueError):
        start_sysinfo_publisher(refresh_interval=float("nan"))


def test_drop_stops_publisher() -> None:
    """The publisher is stopped when the python object is garbage collected."""
    publisher = start_sysinfo_publisher()
    del publisher
