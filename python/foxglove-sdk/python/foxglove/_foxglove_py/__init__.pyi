from pathlib import Path
from typing import Any, List, Optional, Tuple

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

    def schema_name(self) -> Optional[str]:
        """The name of the schema for the channel"""
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
    writer_options: Optional[MCAPWriteOptions] = None,
) -> MCAPWriter:
    """
    Creates a new MCAP file for recording.

    :param path: The path to the MCAP file. This file will be created and must not already exist.
    :param allow_overwrite: Set this flag in order to overwrite an existing file at this path.
    :param writer_options: Options for the MCAP writer.
    :rtype: :py:class:`MCAPWriter`
    """
    ...

def get_channel_for_topic(topic: str) -> BaseChannel:
    """
    Get a previously-registered channel.
    """
    ...
