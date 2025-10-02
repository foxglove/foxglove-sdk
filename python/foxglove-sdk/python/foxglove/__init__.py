"""
This module provides interfaces for logging messages to Foxglove.

See :py:mod:`foxglove.schemas` and :py:mod:`foxglove.channels` for working with well-known Foxglove
schemas.
"""

from __future__ import annotations

import atexit
import logging
from typing import TYPE_CHECKING, Optional

from . import _foxglove_py as _foxglove

# Re-export these imports
from ._foxglove_py import Context, Schema, open_mcap
from .channel import Channel, log

# Deprecated. Use foxglove.mcap.MCAPWriter instead.
from .mcap import MCAPWriter

if TYPE_CHECKING:
    from .notebook.FoxgloveViewer import FoxgloveViewer

atexit.register(_foxglove.shutdown)


try:
    from .websocket import (
        AssetHandler,
        Capability,
        ServerListener,
        Service,
        WebSocketServer,
    )

    def start_server(
        *,
        name: str | None = None,
        host: str | None = "127.0.0.1",
        port: int | None = 8765,
        capabilities: list[Capability] | None = None,
        server_listener: ServerListener | None = None,
        supported_encodings: list[str] | None = None,
        services: list[Service] | None = None,
        asset_handler: AssetHandler | None = None,
        context: Context | None = None,
        session_id: str | None = None,
    ) -> WebSocketServer:
        """
        Start a websocket server for live visualization.

        :param name: The name of the server.
        :param host: The host to bind to.
        :param port: The port to bind to.
        :param capabilities: A list of capabilities to advertise to clients.
        :param server_listener: A Python object that implements the
            :py:class:`websocket.ServerListener` protocol.
        :param supported_encodings: A list of encodings to advertise to clients.
        :param services: A list of services to advertise to clients.
        :param asset_handler: A callback function that returns the asset for a given URI, or None if
            it doesn't exist.
        :param context: The context to use for logging. If None, the global context is used.
        :param session_id: An ID which allows the client to understand if the connection is a
            re-connection or a new server instance. If None, then an ID is generated based on the
            current time.
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
            session_id=session_id,
        )

except ImportError:
    pass


def set_log_level(level: int | str = "INFO") -> None:
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


def create_notebook_buffer(context: Context | None = None) -> None:
    """
    Create a data buffer for collecting messages in Jupyter notebooks. The buffer
    will be associated with the provided context, so every message logged to the context
    will be collected by the buffer.

    Args:
        context: Optional Foxglove context to use for logging. If not provided,
            the global context will be used. This allows you to isolate the
            buffer's data collection from other parts of your application.

    Example:
        >>> import foxglove
        >>> from foxglove.schemas import SceneUpdate
        >>>
        >>> # Create a buffer with the global context
        >>> foxglove.create_notebook_buffer()
        >>>
        >>> # Or create a buffer with a specific context
        >>> context = foxglove.Context()
        >>> foxglove.create_notebook_buffer(context=context)
    """
    try:
        from .notebook.FoxgloveViewer import FoxgloveViewer

    except ImportError:
        raise Exception(
            "FoxgloveViewer is not installed. "
            "Please install it with `pip install foxglove-sdk[notebook]`"
        )

    FoxgloveViewer.create_notebook_buffer(context=context)


def notebook_viewer(
    context: Context | None = None,
    width: Optional[str] = None,
    height: Optional[str] = None,
    src: Optional[str] = None,
    layout_data: Optional[dict] = None,
) -> FoxgloveViewer:
    """
    Create a FoxgloveViewer widget for interactive data visualization in Jupyter notebooks.

    This function creates an embedded Foxglove visualization widget that displays
    the data collected in the NotebookBuffer associated with the provided context.
    The widget provides a fully-featured Foxglove interface directly within
    your Jupyter notebook, allowing you to explore multi-modal robotics data
    including 3D scenes, plots, images, and more.

    Args:
        context: The Context used to log the messages. If no Context is provided, the global
            context will be used. The visualization data will be retrieved from the NotebookBuffer
            associated with the provided context. This buffer should have been populated with
            logged messages before creating the viewer.
        width: Optional width for the widget. Can be specified as CSS values like
            "800px", "100%", "50vw", etc. If not provided, defaults to "100%".
        height: Optional height for the widget. Can be specified as CSS values like
            "600px", "80vh", "400px", etc. If not provided, defaults to "500px".
        src: Optional URL of the Foxglove app instance to use. If not provided or empty,
            uses the default Foxglove embed server (https://embed.foxglove.dev/).
        layout_data: Optional layout data to be used by the Foxglove viewer. Should be a
            dictionary that was exported from the Foxglove app. If not provided, uses the
            default layout.

    Returns:
        FoxgloveViewer: A Jupyter widget that displays the embedded Foxglove
            visualization interface with the provided data.

    Raises:
        Exception: If the notebook extra package is not installed. Install it
            with `pip install foxglove-sdk[notebook]`.

    Note:
        This function is only available when the `notebook` extra package
        is installed. Install it with `pip install foxglove-sdk[notebook]`.
        The widget will automatically load the data from the provided datasource
        and display it in the embedded Foxglove viewer.

    Example:
        >>> import foxglove
        >>> from foxglove.schemas import SceneUpdate
        >>>
        >>> # Create a buffer and log some data
        >>> foxglove.create_notebook_buffer()
        >>> # ... your application logs data to the buffer ...
        >>>
        >>> # Create a basic viewer using the default context
        >>> viewer = foxglove.notebook_viewer()
        >>>
        >>> # Create a custom-sized viewer
        >>> viewer = foxglove.notebook_viewer(
        ...     width="800px",
        ...     height="600px",
        ...     orgSlug="my-org"
        ... )
        >>>
        >>> # Create a viewer using a specific context
        >>> viewer = foxglove.notebook_viewer(context=my_ctx)
        >>>
        >>> # Display the widget in the notebook
        >>> viewer
    """
    try:
        from .notebook.FoxgloveViewer import FoxgloveViewer

    except ImportError:
        raise Exception(
            "FoxgloveViewer is not installed. "
            "Please install it with `pip install foxglove-sdk[notebook]`"
        )

    return FoxgloveViewer(
        context=context,
        width=width,
        height=height,
        src=src,
        layout_data=layout_data,
    )


__all__ = [
    "Channel",
    "Context",
    "MCAPWriter",
    "Schema",
    "create_notebook_buffer",
    "log",
    "open_mcap",
    "set_log_level",
    "start_server",
    "notebook_viewer",
]
