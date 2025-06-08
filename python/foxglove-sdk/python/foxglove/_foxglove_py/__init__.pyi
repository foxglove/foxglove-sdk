from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

from .mcap import MCAPWriteOptions, MCAPWriter
from .websocket import AssetHandler, Capability, Service, WebSocketServer

class BaseChannel:
    """
    A channel for logging messages.
    """

    def __new__(
        cls,
        topic: str,
        message_encoding: str,
        schema: Optional["Schema"] = None,
        metadata: Optional[List[Tuple[str, str]]] = None,
    ) -> "BaseChannel": ...
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

    def metadata(self) -> Dict[str, str]:
        """
        Returns a copy of the channel's metadata.

        Note that changes made to the returned dictionary will not be applied to
        the channel's metadata.
        """
        ...

    def schema(self) -> Optional["Schema"]:
        """
        Returns a copy of the channel's schema.

        Note that changes made to the returned object will not be applied to
        the channel's schema.
        """
        ...

    def schema_name(self) -> Optional[str]:
        """The name of the schema for the channel"""
        ...

    def has_sinks(self) -> bool:
        """Returns true if at least one sink is subscribed to this channel"""
        ...

    def log(
        self,
        msg: bytes,
        log_time: Optional[int] = None,
    ) -> None:
        """
        Log a message to the channel.

        :param msg: The message to log.
        :param log_time: The optional time the message was logged.
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

    def __new__(
        cls,
        *,
        name: str,
        encoding: str,
        data: bytes,
    ) -> "Schema": ...

class Context:
    """
    A context for logging messages.

    A context is the binding between channels and sinks. By default, the SDK will use a single
    global context for logging, but you can create multiple contexts in order to log to different
    topics to different sinks or servers. To do so, associate the context by passing it to the
    channel constructor and to :py:func:`open_mcap` or :py:func:`start_server`.
    """

    def __new__(cls) -> "Context": ...
    def _create_channel(
        self,
        topic: str,
        message_encoding: str,
        schema: Optional["Schema"] = None,
        metadata: Optional[List[Tuple[str, str]]] = None,
    ) -> "BaseChannel":
        """
        Instead of calling this method, pass a context to a channel constructor.
        """
        ...

def start_server(
    *,
    name: Optional[str] = None,
    host: Optional[str] = "127.0.0.1",
    port: Optional[int] = 8765,
    capabilities: Optional[List[Capability]] = None,
    server_listener: Any = None,
    supported_encodings: Optional[List[str]] = None,
    services: Optional[List["Service"]] = None,
    asset_handler: Optional["AssetHandler"] = None,
    context: Optional["Context"] = None,
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
    context: Optional["Context"] = None,
    writer_options: Optional[MCAPWriteOptions] = None,
) -> MCAPWriter:
    """
    Creates a new MCAP file for recording.

    If a context is provided, the MCAP file will be associated with that context. Otherwise, the
    global context will be used.
    """
    ...

def get_channel_for_topic(topic: str) -> Optional[BaseChannel]:
    """
    Get a previously-registered channel.
    """
    ...
