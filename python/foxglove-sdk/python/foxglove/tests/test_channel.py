import logging
import random

import pytest
from foxglove import Context, Schema
from foxglove.channel import Channel
from foxglove.channels import LogChannel
from foxglove.schemas import Log


@pytest.fixture
def new_topic() -> str:
    return f"/{random.random()}"


def test_warns_on_duplicate_topics(caplog: pytest.LogCaptureFixture) -> None:
    schema = {"type": "object"}
    c1 = Channel("test-duplicate", schema=schema)
    c2 = Channel("test-duplicate", schema=schema)
    assert c1.id() == c2.id()

    with caplog.at_level(logging.WARNING):
        # Same topic, different schema
        c3 = Channel(
            "test-duplicate",
            schema={
                "type": "object",
                "additionalProperties": False,
            },
        )
        assert c1.id() != c3.id()

    assert len(caplog.records) == 1
    for _, _, message in caplog.record_tuples:
        assert (
            "Channel with topic test-duplicate already exists in this context"
            in message
        )


def test_does_not_warn_on_duplicate_topics_in_contexts(
    caplog: pytest.LogCaptureFixture,
) -> None:
    ctx1 = Context()
    ctx2 = Context()

    _ = Channel("test-duplicate", context=ctx1)

    with caplog.at_level(logging.WARNING):
        Channel("test-duplicate", context=ctx2)

    assert len(caplog.records) == 0


def test_requires_an_object_schema(new_topic: str) -> None:
    schema = {"type": "array"}
    with pytest.raises(ValueError, match="Only object schemas are supported"):
        Channel(new_topic, schema=schema)


def test_log_dict_on_json_channel(new_topic: str) -> None:
    channel = Channel(
        new_topic,
        schema={"type": "object", "additionalProperties": True},
    )
    assert channel.message_encoding == "json"

    channel.log({"test": "test"})


def test_log_dict_on_schemaless_channel(new_topic: str) -> None:
    channel = Channel(new_topic)
    assert channel.message_encoding == "json"

    channel.log({"test": "test"})


def test_log_dict_with_empty_schema(new_topic: str) -> None:
    channel = Channel(new_topic, schema={})
    assert channel.message_encoding == "json"

    channel.log({"test": "test"})


def test_log_dict_on_schemaless_json_channel(new_topic: str) -> None:
    channel = Channel(
        new_topic,
        message_encoding="json",
    )
    assert channel.message_encoding == "json"

    channel.log({"test": "test"})


def test_log_must_serialize_on_protobuf_channel(new_topic: str) -> None:
    channel = Channel(
        new_topic,
        message_encoding="protobuf",
        schema=Schema(
            name="my_schema",
            encoding="protobuf",
            data=b"\x01",
        ),
    )

    with pytest.raises(TypeError, match="Unsupported message type"):
        channel.log({"test": "test"})

    channel.log(b"\x01")


def test_closed_channel_log(new_topic: str, caplog: pytest.LogCaptureFixture) -> None:
    channel = Channel(new_topic, schema={"type": "object"})
    channel.close()
    with caplog.at_level(logging.WARNING):
        channel.log(b"\x01")

    assert len(caplog.records) == 1
    for log_name, _, message in caplog.record_tuples:
        assert log_name == "foxglove.channel.raw_channel"
        assert message == f"Cannot log on closed channel for {new_topic}"


def test_close_typed_channel(new_topic: str, caplog: pytest.LogCaptureFixture) -> None:
    channel = LogChannel(new_topic)
    channel.close()
    with caplog.at_level(logging.WARNING):
        channel.log(Log())

    assert len(caplog.records) == 1
    for log_name, _, message in caplog.record_tuples:
        assert log_name == "foxglove.channel.raw_channel"
        assert message == f"Cannot log on closed channel for {new_topic}"


def test_typed_channel_requires_kwargs_after_message(new_topic: str) -> None:
    channel = LogChannel(new_topic)

    channel.log(Log(), log_time=0)

    with pytest.raises(
        TypeError,
        match="takes 1 positional arguments but 2 were given",
    ):
        channel.log(Log(), 0)  # type: ignore


def test_generates_names_for_schemas(new_topic: str) -> None:
    ch_1 = Channel(
        new_topic + "-1",
        schema={"type": "object", "properties": {"foo": {"type": "string"}}},
    )
    ch_2 = Channel(
        new_topic + "-2",
        schema={"type": "object", "additionalProperties": True},
    )
    # Same schema will have the same name
    ch_3 = Channel(
        new_topic + "-3",
        schema={"type": "object", "additionalProperties": True},
    )

    assert ch_1.schema_name() != ch_2.schema_name()
    assert ch_2.schema_name() == ch_3.schema_name()


def test_exposes_unique_channel_ids(new_topic: str) -> None:
    ch_1 = Channel(new_topic + "-1")
    ch_2 = Channel(new_topic + "-2")
    ch_3 = LogChannel(new_topic + "-3")

    assert ch_1.id() > 0
    assert ch_1.id() < ch_2.id()
    assert ch_2.id() < ch_3.id()
