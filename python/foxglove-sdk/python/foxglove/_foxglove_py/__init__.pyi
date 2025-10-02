from pathlib import Path
from typing import Any

from foxglove.websocket import AssetHandler

from .mcap import MCAPWriteOptions, MCAPWriter
from .websocket import Capability, Service, WebSocketServer

class BaseChannel:
    """
    A channel for logging messages.
    """

    def __init__(
        self,
        topic: str,
        message_encoding: str,
        schema: Schema | None = None,
        metadata: dict[str, str] | None = None,
    ) -> None: ...
    def id(self) -> int:
        """The unique ID of the channel"""
        ...

    def topic(self) -> str:
        """The topic name of the channel"""
        ...

    @property
    def message_encoding(self) -> str:
        """The message encoding for the channel"""
        ...

    def metadata(self) -> dict[str, str]:
        """
        Returns a copy of the channel's metadata.

        Note that changes made to the returned dictionary will not be applied to
        the channel's metadata.
        """
        ...

    def schema(self) -> "Schema" | None:
        """
        Returns a copy of the channel's schema.

        Note that changes made to the returned object will not be applied to
        the channel's schema.
        """
        ...

    def schema_name(self) -> str | None:
        """The name of the schema for the channel"""
        ...

    def has_sinks(self) -> bool:
        """Returns true if at least one sink is subscribed to this channel"""
        ...

    def log(
        self,
        msg: bytes,
        log_time: int | None = None,
        sink_id: int | None = None,
    ) -> None:
        """
        Log a message to the channel.

        :param msg: The message to log.
        :param log_time: The optional time the message was logged.
        :param sink_id: The sink ID to log the message to. If not provided, the message will be
            sent to all sinks.
        """
        ...

    def close(self) -> None: ...

class Schema:
    """
    A schema for a message or service call.
    """

    name: str
    encoding: str
    data: bytes

    def __init__(
        self,
        *,
        name: str,
        encoding: str,
        data: bytes,
    ) -> None: ...

class Context:
    """
    A context for logging messages.

    A context is the binding between channels and sinks. By default, the SDK will use a single
    global context for logging, but you can create multiple contexts in order to log to different
    topics to different sinks or servers. To do so, associate the context by passing it to the
    channel constructor and to :py:func:`open_mcap` or :py:func:`start_server`.
    """

    def __init__(self) -> None: ...
    def _create_channel(
        self,
        topic: str,
        message_encoding: str,
        schema: Schema | None = None,
        metadata: list[tuple[str, str]] | None = None,
    ) -> "BaseChannel":
        """
        Instead of calling this method, pass a context to a channel constructor.
        """
        ...

    @staticmethod
    def default() -> "Context":
        """
        Returns the default context.
        """
        ...

def start_server(
    *,
    name: str | None = None,
    host: str | None = "127.0.0.1",
    port: int | None = 8765,
    capabilities: list[Capability] | None = None,
    server_listener: Any = None,
    supported_encodings: list[str] | None = None,
    services: list[Service] | None = None,
    asset_handler: AssetHandler | None = None,
    context: Context | None = None,
    session_id: str | None = None,
) -> WebSocketServer:
    """
    Start a websocket server for live visualization.
    """
    ...

def enable_logging(level: int) -> None:
    """
    Forward SDK logs to python's logging facility.
    """
    ...

def disable_logging() -> None:
    """
    Stop forwarding SDK logs.
    """
    ...

def shutdown() -> None:
    """
    Shutdown the running websocket server.
    """
    ...

def open_mcap(
    path: str | Path,
    *,
    allow_overwrite: bool = False,
    context: Context | None = None,
    writer_options: MCAPWriteOptions | None = None,
) -> MCAPWriter:
    """
    Creates a new MCAP file for recording.

    If a context is provided, the MCAP file will be associated with that context. Otherwise, the
    global context will be used.
    """
    ...

def get_channel_for_topic(topic: str) -> BaseChannel | None:
    """
    Get a previously-registered channel.
    """
    ...
