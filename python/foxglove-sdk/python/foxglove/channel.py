import json
from typing import Any, Dict, Optional, Union, cast

from . import _foxglove_py as _foxglove
from . import channels as _channels
from . import schemas as _schemas

JsonSchema = Dict[str, Any]
JsonMessage = Dict[str, Any]


class Channel:
    """
    A channel that can be used to log binary messages or JSON messages.
    """

    __slots__ = ["base", "message_encoding"]
    base: _foxglove.BaseChannel
    message_encoding: str

    def __init__(
        self,
        topic: str,
        *,
        schema: Union[JsonSchema, _foxglove.Schema, None] = None,
        message_encoding: Optional[str] = None,
    ):
        """
        Create a new channel for logging messages on a topic.

        :param topic: The topic name.
        :param message_encoding: The message encoding. Optional if
            :py:param:`schema` is a dictionary, in which case the message
            encoding is presumed to be "json".
        :param schema: A definition of your schema. Pass a :py:class:`Schema`
            for full control. If a dictionary is passed, it will be treated as a
            JSON schema.

        :raises KeyError: if a channel already exists for the given topic.
        """
        if topic in _channels_by_topic:
            raise ValueError(f"Channel for topic '{topic}' already exists")

        message_encoding, schema = _normalize_schema(message_encoding, schema)

        self.message_encoding = message_encoding

        self.base = _foxglove.BaseChannel(
            topic,
            message_encoding,
            schema,
        )

        _channels_by_topic[topic] = self

    def log(
        self,
        msg: Union[JsonMessage, list[Any], bytes, str],
        *,
        log_time: Optional[int] = None,
        publish_time: Optional[int] = None,
        sequence: Optional[int] = None,
    ) -> None:
        """
        Log a message on the channel.

        :param msg: the message to log. If the channel uses JSON encoding, you may pass a
            dictionary or list. Otherwise, you are responsible for serializing the message.
        """
        if self.message_encoding == "json" and isinstance(msg, (dict, list)):
            return self.base.log(
                json.dumps(msg).encode("utf-8"), log_time, publish_time, sequence
            )

        if isinstance(msg, str):
            msg = msg.encode("utf-8")
        if isinstance(msg, bytes):
            return self.base.log(msg, log_time, publish_time, sequence)

        raise TypeError(f"Unsupported message type: {type(msg)}")

    def close(self) -> None:
        """
        Close the channel.

        You do not need to call this unless you explicitly want to remove advertisements from live
        visualization clients. Destroying all references to the channel will also close it.
        """
        self.base.close()


_channels_by_topic: Dict[str, Channel] = {}


def log(
    topic: str,
    message: Union[JsonMessage, list[Any], bytes, str, _schemas.FoxgloveSchema],
    *,
    log_time: Optional[int] = None,
    publish_time: Optional[int] = None,
    sequence: Optional[int] = None,
) -> None:
    """Log a message on a topic.

    Creates a new channel the first time called for a given topic.
    For Foxglove types in the schemas module, this creates a typed channel
    (see :py:mod:`foxglove.channels` for supported types).
    For bytes and str, this creates a simple schemaless channel and logs the bytes as-is.
    For dict and list, this creates a schemaless json channel.

    The type of the message must be kept consistent for each topic or an error will be raised.
    This can be avoided by creating and using the channels directly instead of using this function.

    Note: this raises an error if a channel with the same topic was created by other means.
    This limitation may be lifted in the future.

    :param topic: The topic name.
    :param message: The message to log.
    """
    channel: Optional[Any] = _channels_by_topic.get(topic, None)
    if channel is None:
        schema_name = type(message).__name__
        if isinstance(message, (bytes, str)):
            channel = Channel(topic)
        elif isinstance(message, (dict, list)):
            channel = Channel(topic, message_encoding="json")
        else:
            channel_name = f"{schema_name}Channel"
            channel_cls = getattr(_channels, channel_name, None)
            if channel_cls is not None:
                channel = channel_cls(topic)
        if channel is None:
            raise ValueError(
                f"No Foxglove schema channel found for message type {schema_name}"
            )
        _channels_by_topic[topic] = channel

    # mypy isn't smart enough to realize that when channel is a Channel, message a compatible type
    channel.log(
        cast(Any, message),
        log_time=log_time,
        publish_time=publish_time,
        sequence=sequence,
    )


def _normalize_schema(
    message_encoding: Optional[str],
    schema: Union[JsonSchema, _foxglove.Schema, None] = None,
) -> tuple[str, Optional[_foxglove.Schema]]:
    if isinstance(schema, _foxglove.Schema) or schema is None:
        if message_encoding is None:
            raise ValueError("message encoding is required")
        return message_encoding, schema
    elif isinstance(schema, dict):
        if schema.get("type") != "object":
            raise ValueError("Only object schemas are supported")
        return (
            message_encoding or "json",
            _foxglove.Schema(
                name=schema.get("title", "json_schema"),
                encoding="jsonschema",
                data=json.dumps(schema).encode("utf-8"),
            ),
        )
    else:
        raise TypeError(f"Invalid schema type: {type(schema)}")
