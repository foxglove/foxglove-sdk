import json
from typing import Any, Dict, Optional, Union

from ._foxglove_py import BaseChannel, Schema, channels

JsonSchema = Dict[str, Any]
JsonMessage = Dict[str, Any]


class Channel:
    """
    A channel that can be used to log binary messages or JSON messages.
    """

    __slots__ = ["base", "message_encoding"]
    base: BaseChannel
    message_encoding: str

    def __init__(
        self,
        topic: str,
        *,
        schema: Union[JsonSchema, Schema, None],
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
            JSON schema. If not provided, the message_encoding must be "json".

        :raises KeyError: if a channel already exists for the given topic.
        """
        if topic in _channels_by_topic:
            raise ValueError(f"Channel for topic '{topic}' already exists")

        message_encoding, schema = _normalize_schema(message_encoding, schema)

        self.message_encoding = message_encoding

        self.base = BaseChannel(
            topic,
            message_encoding,
            schema,
        )

        _channels_by_topic[topic] = self

    def log(
        self,
        msg: Union[JsonMessage, bytes],
        log_time: Optional[int] = None,
        publish_time: Optional[int] = None,
        sequence: Optional[int] = None,
    ) -> None:
        """
        Log a message on the channel.

        :param msg: the message to log. If the channel uses JSON encoding, you may pass a
            dictionary. Otherwise, you are responsible for serializing the message.
        """
        if isinstance(msg, bytes):
            return self.base.log(msg, log_time, publish_time, sequence)

        if self.message_encoding == "json":
            return self.base.log(
                json.dumps(msg).encode("utf-8"), log_time, publish_time, sequence
            )

        raise ValueError(f"Unsupported message type: {type(msg)}")

    def close(self) -> None:
        """
        Close the channel.

        You do not need to call this unless you explicitly want to remove advertisements from live
        visualization clients. Destroying all references to the channel will also close it.
        """
        self.base.close()


_channels_by_topic: Dict[str, Channel] = {}


def log(topic: str, message: Any) -> None:
    channel: Optional[Channel] = _channels_by_topic.get(topic, None)
    if channel is None:
        schema_name = type(message).__name__
        channel_name = f"{schema_name}Channel"
        channel_cls = getattr(channels, channel_name, None)
        if channel_cls is not None:
            channel = channel_cls(topic)
        if channel is None:
            raise ValueError(
                f"No Foxglove schema channel found for message type {type(message).__name__}"
            )
        _channels_by_topic[topic] = channel
    else:
        # TODO: Check schema compatibility with proto_msg
        pass

    channel.log(message)


def _normalize_schema(
    message_encoding: Optional[str],
    schema: Union[JsonSchema, Schema, None] = None,
) -> tuple[str, Optional[Schema]]:
    if isinstance(schema, Schema) or schema is None:
        if message_encoding is None:
            raise ValueError("message encoding is required")
        return message_encoding, schema
    elif isinstance(schema, dict):
        if schema.get("type") != "object":
            raise ValueError("Only object schemas are supported")
        return (
            message_encoding or "json",
            Schema(
                name=schema.get("title", "json_schema"),
                encoding="jsonschema",
                data=json.dumps(schema).encode("utf-8"),
            ),
        )
    else:
        raise ValueError(f"Invalid schema type: {type(schema)}")
