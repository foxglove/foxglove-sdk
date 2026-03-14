"""Verify that foxglove.messages is the canonical module for Foxglove message types."""

import importlib
import warnings

import foxglove.messages


def test_all_message_types_exported() -> None:
    """Every name in __all__ should be available in foxglove.messages."""
    for name in foxglove.messages.__all__:
        assert hasattr(
            foxglove.messages, name
        ), f"{name} missing from foxglove.messages"


def test_schemas_emits_deprecation_warning() -> None:
    """Importing foxglove.schemas should emit a DeprecationWarning."""
    with warnings.catch_warnings(record=True) as w:
        warnings.simplefilter("always")

        # The module may already be cached, so force a reload to trigger the warning.
        import foxglove.schemas  # noqa: F401

        importlib.reload(foxglove.schemas)

        deprecation_warnings = [
            x for x in w if issubclass(x.category, DeprecationWarning)
        ]
        assert (
            len(deprecation_warnings) > 0
        ), "Expected a DeprecationWarning when importing foxglove.schemas"
        assert "foxglove.messages" in str(deprecation_warnings[0].message)


def test_schemas_reexports_same_types() -> None:
    """foxglove.schemas should still provide the same types as foxglove.messages."""
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", DeprecationWarning)
        import foxglove.schemas

    for name in foxglove.messages.__all__:
        if name in ("FoxgloveMessage", "FoxgloveSchema"):
            # Union type aliases, separately constructed in each module.
            continue
        assert getattr(foxglove.messages, name) is getattr(
            foxglove.schemas, name
        ), f"{name} in foxglove.messages is not the same object as in foxglove.schemas"


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
