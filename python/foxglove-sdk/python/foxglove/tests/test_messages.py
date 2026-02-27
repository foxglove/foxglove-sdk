"""Verify that foxglove.messages re-exports everything from foxglove.schemas."""

import foxglove.messages
import foxglove.schemas


def test_all_schemas_exported_from_messages() -> None:
    """Every name in foxglove.schemas.__all__ should be available in foxglove.messages."""
    for name in foxglove.schemas.__all__:
        assert hasattr(foxglove.messages, name), f"{name} missing from foxglove.messages"


def test_objects_are_identical() -> None:
    """Exported objects should be the exact same objects, not copies."""
    for name in foxglove.schemas.__all__:
        assert getattr(foxglove.messages, name) is getattr(foxglove.schemas, name), (
            f"{name} in foxglove.messages is not the same object as in foxglove.schemas"
        )


def test_messages_can_construct_types() -> None:
    """Types imported from foxglove.messages should work normally."""
    from foxglove.messages import Log, LogLevel, Timestamp

    msg = Log(
        timestamp=Timestamp(5, 10),
        level=LogLevel.Error,
        message="hello",
        name="logger",
        file="file",
        line=123,
    )
    encoded = msg.encode()
    assert isinstance(encoded, bytes)
    assert len(encoded) == 34
