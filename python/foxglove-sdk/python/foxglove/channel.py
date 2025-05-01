import hashlib
import json
from base64 import b64encode
from typing import Any, Dict, Optional, Union, cast

from . import Context
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
        context: Optional[Context] = None,
    ):
        """
        Create a new channel for logging messages on a topic.

        :param topic: The topic name. You should choose a unique topic name per channel.
        :param message_encoding: The message encoding. Optional if
            :py:param:`schema` is a dictionary, in which case the message
            encoding is presumed to be "json".
        :param schema: A definition of your schema. Pass a :py:class:`Schema`
            for full control. If a dictionary is passed, it will be treated as a
            JSON schema.

        If both message_encoding and schema are None, then the channel will use JSON encoding, and
        allow any dict to be logged.
        """
        message_encoding, schema = _normalize_schema(message_encoding, schema)

        self.message_encoding = message_encoding

        if context is not None:
            self.base = context._create_channel(
                topic, message_encoding=message_encoding, schema=schema
            )
        else:
            self.base = _foxglove.BaseChannel(
                topic,
                message_encoding,
                schema,
            )

        _channels_by_id[self.base.id()] = self

    def __repr__(self) -> str:
        return f"Channel(id={self.id()}, topic='{self.topic()}', schema='{self.schema_name()}')"

    def id(self) -> int:
        """The unique ID of the channel"""
        return self.base.id()

    def topic(self) -> str:
        """The topic name of the channel"""
        return self.base.topic()

    def schema_name(self) -> Optional[str]:
        """The name of the schema for the channel"""
        return self.base.schema_name()

    def log(
        self,
        msg: Union[JsonMessage, list[Any], bytes, str],
        *,
        log_time: Optional[int] = None,
    ) -> None:
        """
        Log a message on the channel.

        :param msg: the message to log. If the channel uses JSON encoding, you may pass a
            dictionary or list. Otherwise, you are responsible for serializing the message.
        :param log_time: The optional time the message was logged.
        """
        if self.message_encoding == "json" and isinstance(msg, (dict, list)):
            return self.base.log(json.dumps(msg).encode("utf-8"), log_time)

        if isinstance(msg, str):
            msg = msg.encode("utf-8")

        if isinstance(msg, bytes):
            return self.base.log(msg, log_time)

        raise TypeError(f"Unsupported message type: {type(msg)}")

    def close(self) -> None:
        """
        Close the channel.

        You can use this to explicitly unadvertise the channel to sinks that subscribe to
        channels dynamically, such as the :py:class:`foxglove.websocket.WebSocketServer`.

        Attempts to log on a closed channel will elicit a throttled warning message.
        """
        self.base.close()


_channels_by_id: Dict[int, Channel] = {}


def log(
    topic: str,
    message: Union[JsonMessage, list[Any], bytes, str, _schemas.FoxgloveSchema],
    *,
    log_time: Optional[int] = None,
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
    :param log_time: The optional time the message was logged.
    """
    base_channel = _foxglove.get_channel_for_topic(topic)
    channel = _channels_by_id.get(base_channel.id(), None) if base_channel else None

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

        channel_id = channel.base.id() if hasattr(channel, "base") else channel.id()
        _channels_by_id[channel_id] = channel

    # mypy isn't smart enough to realize that when channel is a Channel, message a compatible type
    channel.log(
        cast(Any, message),
        log_time=log_time,
    )


def _normalize_schema(
    message_encoding: Optional[str],
    schema: Union[JsonSchema, _foxglove.Schema, None] = None,
) -> tuple[str, Optional[_foxglove.Schema]]:
    if isinstance(schema, _foxglove.Schema):
        if message_encoding is None:
            raise ValueError("message encoding is required")
        return message_encoding, schema

    if schema is None and (message_encoding is None or message_encoding == "json"):
        # Schemaless support via JSON encoding; same as specifying an empty dict schema
        schema = {}
        message_encoding = "json"

    if isinstance(schema, dict):
        # Dicts default to json encoding. An empty dict is equivalent to the empty schema (b"")
        if message_encoding and message_encoding != "json":
            raise ValueError("message_encoding must be 'json' when schema is a dict")
        if schema and schema.get("type") != "object":
            raise ValueError("Only object schemas are supported")

        data = json.dumps(schema).encode("utf-8") if schema else b""
        name = schema["title"] if "title" in schema else _default_schema_name(data)

        return (
            "json",
            _foxglove.Schema(
                name=name,
                encoding="jsonschema",
                data=data,
            ),
        )

    raise TypeError(f"Invalid schema type: {type(schema)}")


def _default_schema_name(data: bytes) -> str:
    # Provide a consistent, readable, and reasonably unique name for a given schema so the app can
    # identify it to the user.
    hash = hashlib.shake_128(data).digest(6)
    return "schema-" + b64encode(hash, b"-_").decode("utf-8")
