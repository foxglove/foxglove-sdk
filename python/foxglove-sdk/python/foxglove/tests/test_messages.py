"""
Tests for the `foxglove.messages` module and backward-compatible `foxglove.schemas` module.

The messages module contains well-known Foxglove message types for logging. The schemas
module is deprecated and re-exports from messages for backward compatibility.
"""

import warnings

import pytest

from foxglove.messages import Log, LogLevel, Timestamp


class TestMessagesModule:
    """Tests for the foxglove.messages module."""

    def test_can_encode_log_message(self) -> None:
        """Verify Log message can be encoded as protobuf."""
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

    def test_timestamp_fields(self) -> None:
        """Verify Timestamp fields are accessible."""
        ts = Timestamp(123, 456)
        assert ts.sec == 123
        assert ts.nsec == 456

    def test_log_level_enum(self) -> None:
        """Verify LogLevel enum values."""
        assert LogLevel.Debug.value == 1
        assert LogLevel.Info.value == 2
        assert LogLevel.Warning.value == 3
        assert LogLevel.Error.value == 4
        assert LogLevel.Fatal.value == 5

    def test_get_schema(self) -> None:
        """Verify `get_schema()` returns a valid schema."""
        schema = Log.get_schema()
        assert schema.name == "foxglove.Log"
        assert schema.encoding == "protobuf"
        assert len(schema.data) > 0


class TestSchemasDeprecation:
    """Tests for the deprecated foxglove.schemas module."""

    def test_schemas_import_emits_deprecation_warning(self) -> None:
        """Importing foxglove.schemas should emit a DeprecationWarning."""
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            # Force reimport by removing from cache.
            import sys

            if "foxglove.schemas" in sys.modules:
                del sys.modules["foxglove.schemas"]

            import foxglove.schemas  # noqa: F401

            # Check that a DeprecationWarning was raised.
            deprecation_warnings = [
                x for x in w if issubclass(x.category, DeprecationWarning)
            ]
            assert len(deprecation_warnings) >= 1
            assert "foxglove.schemas is deprecated" in str(
                deprecation_warnings[0].message
            )
            assert "foxglove.messages" in str(deprecation_warnings[0].message)

    def test_schemas_exports_same_types_as_messages(self) -> None:
        """The schemas module should export the same types as messages."""
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", DeprecationWarning)
            import foxglove.schemas as schemas_mod

        import foxglove.messages as messages_mod

        # Verify key types are the same.
        assert schemas_mod.Log is messages_mod.Log
        assert schemas_mod.Timestamp is messages_mod.Timestamp
        assert schemas_mod.LogLevel is messages_mod.LogLevel
        assert schemas_mod.CompressedImage is messages_mod.CompressedImage
        assert schemas_mod.SceneUpdate is messages_mod.SceneUpdate

    def test_schemas_types_work_correctly(self) -> None:
        """Types imported from schemas should work the same as from messages."""
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", DeprecationWarning)
            from foxglove.schemas import Log as SchemasLog
            from foxglove.schemas import LogLevel as SchemasLogLevel
            from foxglove.schemas import Timestamp as SchemasTimestamp

        msg = SchemasLog(
            timestamp=SchemasTimestamp(1, 2),
            level=SchemasLogLevel.Info,
            message="test",
        )
        encoded = msg.encode()
        assert isinstance(encoded, bytes)
        assert len(encoded) > 0

    def test_schemas_all_exports_match_messages(self) -> None:
        """The __all__ export list should match between modules."""
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", DeprecationWarning)
            import foxglove.schemas as schemas_mod

        import foxglove.messages as messages_mod

        assert set(schemas_mod.__all__) == set(messages_mod.__all__)
