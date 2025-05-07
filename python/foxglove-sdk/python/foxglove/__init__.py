"""
This module provides interfaces for logging messages to Foxglove.

See :py:mod:`foxglove.schemas` and :py:mod:`foxglove.channels` for working with well-known Foxglove
schemas.
"""

import atexit
import logging
from typing import List, Optional, Union

from . import _foxglove_py as _foxglove

# Re-export these imports
from ._foxglove_py import Context, Schema, open_mcap
from .channel import Channel, log

# Deprecated. Use foxglove.mcap.MCAPWriter instead.
from .mcap import MCAPWriter
from .websocket import (
    AssetHandler,
    Capability,
    ServerListener,
    Service,
    WebSocketServer,
)

atexit.register(_foxglove.shutdown)


def start_server(
    *,
    name: Optional[str] = None,
    host: Optional[str] = "127.0.0.1",
    port: Optional[int] = 8765,
    capabilities: Optional[List[Capability]] = None,
    server_listener: Optional[ServerListener] = None,
    supported_encodings: Optional[List[str]] = None,
    services: Optional[List[Service]] = None,
    asset_handler: Optional[AssetHandler] = None,
    context: Optional[Context] = None,
) -> WebSocketServer:
    """
    Start a websocket server for live visualization.

    :param name: The name of the server.
    :param host: The host to bind to.
    :param port: The port to bind to.
    :param capabilities: A list of capabilities to advertise to clients.
    :param server_listener: A Python object that implements the :py:class:`websocket.ServerListener`
        protocol.
    :param supported_encodings: A list of encodings to advertise to clients.
    :param services: A list of services to advertise to clients.
    :param asset_handler: A callback function that returns the asset for a given URI, or None if
        it doesn't exist.
    :param context: The context to use for logging. If None, the global context is used.
    """
    return _foxglove.start_server(
        name=name,
        host=host,
        port=port,
        capabilities=capabilities,
        server_listener=server_listener,
        supported_encodings=supported_encodings,
        services=services,
        asset_handler=asset_handler,
        context=context,
    )


def set_log_level(level: Union[int, str] = "INFO") -> None:
    """
    Enable SDK logging.

    This function will call logging.basicConfig() for convenience in scripts, but in general you
    should configure logging yourself before calling this function:
    https://docs.python.org/3/library/logging.html

    :param level: The logging level to set. This accepts the same values as `logging.setLevel` and
        defaults to "INFO". The SDK will not log at levels "CRITICAL" or higher.
    """
    # This will raise a ValueError for invalid levels if the user has not already configured
    logging.basicConfig(level=level, format="%(asctime)s [%(levelname)s] %(message)s")

    if isinstance(level, str):
        level_map = (
            logging.getLevelNamesMapping()
            if hasattr(logging, "getLevelNamesMapping")
            else _level_names()
        )
        try:
            level = level_map[level]
        except KeyError:
            raise ValueError(f"Unknown log level: {level}")
    else:
        level = max(0, min(2**32 - 1, level))

    _foxglove.enable_logging(level)


def _level_names() -> dict[str, int]:
    # Fallback for Python <3.11; no support for custom levels
    return {
        "CRITICAL": logging.CRITICAL,
        "FATAL": logging.FATAL,
        "ERROR": logging.ERROR,
        "WARN": logging.WARNING,
        "WARNING": logging.WARNING,
        "INFO": logging.INFO,
        "DEBUG": logging.DEBUG,
        "NOTSET": logging.NOTSET,
    }


__all__ = [
    "Channel",
    "Context",
    "MCAPWriter",
    "Schema",
    "log",
    "open_mcap",
    "set_log_level",
    "start_server",
]
