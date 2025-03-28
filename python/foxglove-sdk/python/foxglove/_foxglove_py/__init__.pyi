from pathlib import Path
from typing import Any, List, Optional, Tuple

from .websocket import AssetHandler, Capability, Service, WebSocketServer

class MCAPWriter:
    """
    A writer for logging messages to an MCAP file.

    Obtain an instance by calling :py:func:`open_mcap`.

    This class may be used as a context manager, in which case the writer will
    be closed when you exit the context.

    If the writer is not closed by the time it is garbage collected, it will be
    closed automatically, and any errors will be logged.
    """

    def __new__(cls) -> "MCAPWriter": ...
    def __enter__(self) -> "MCAPWriter": ...
    def __exit__(self, exc_type: Any, exc_value: Any, traceback: Any) -> None: ...
    def close(self) -> None:
        """
        Close the writer explicitly.

        You may call this to explicitly close the writer. Note that the writer
        will be automatically closed when it is garbage-collected, or when
        exiting the context manager.
        """
        ...

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
    def log(
        self,
        msg: bytes,
        publish_time: Optional[int] = None,
        log_time: Optional[int] = None,
        sequence: Optional[int] = None,
    ) -> None: ...
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

def open_mcap(path: str | Path, allow_overwrite: bool = False) -> MCAPWriter:
    """
    Creates a new MCAP file for recording.

    :param path: The path to the MCAP file. This file will be created and must not already exist.
    :param allow_overwrite: Set this flag in order to overwrite an existing file at this path.
    :rtype: :py:class:`MCAPWriter`
    """
    ...

def get_channel_for_topic(topic: str) -> BaseChannel:
    """
    Get a previously-registered channel.
    """
    ...
