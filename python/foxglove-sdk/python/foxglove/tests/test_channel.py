import random

import pytest
from foxglove import Schema
from foxglove.channel import Channel
from foxglove.channels import LogChannel
from foxglove.schemas import Log


@pytest.fixture
def new_topic() -> str:
    return f"/{random.random()}"


def test_prohibits_duplicate_topics() -> None:
    schema = {"type": "object"}
    _ = Channel("test-duplicate", schema=schema)
    with pytest.raises(ValueError, match="already exists"):
        Channel("test-duplicate", schema=schema)


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
    with caplog.at_level("DEBUG"):
        channel.log(b"\x01")

    assert len(caplog.records) == 1
    for log_name, _, message in caplog.record_tuples:
        assert log_name == "foxglove.channels"
        assert message == "Cannot log() on a closed channel"


def test_close_typed_channel(new_topic: str, caplog: pytest.LogCaptureFixture) -> None:
    channel = LogChannel(new_topic)
    channel.close()
    with caplog.at_level("DEBUG"):
        channel.log(Log())

    assert len(caplog.records) == 1
    for log_name, _, message in caplog.record_tuples:
        assert log_name == "foxglove.channels"
        assert message == "Cannot log() on a closed LogChannel"


def test_typed_channel_requires_kwargs_after_message(new_topic: str) -> None:
    channel = LogChannel(new_topic)

    channel.log(Log(), log_time=0)

    with pytest.raises(
        TypeError,
        match="takes 1 positional arguments but 2 were given",
    ):
        channel.log(Log(), 0)  # type: ignore
