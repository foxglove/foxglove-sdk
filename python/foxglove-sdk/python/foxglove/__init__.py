"""
This module provides interfaces for logging messages to Foxglove.

See :py:mod:`foxglove.messages` and :py:mod:`foxglove.channels` for working with well-known Foxglove
message types.
"""

from __future__ import annotations

import atexit
import logging
from collections.abc import Callable
from typing import TYPE_CHECKING, TypeAlias, Union

from . import _foxglove_py as _foxglove

# Re-export these imports
from ._foxglove_py import (
    ChannelDescriptor,
    ConnectionGraph,
    Context,
    MessageSchema,
    Parameter,
    ParameterType,
    ParameterValue,
    Schema,
    Service,
    ServiceRequest,
    ServiceSchema,
    SinkChannelFilter,
    StatusLevel,
    open_mcap,
)
from .channel import Channel, log

ServiceHandler: TypeAlias = Callable[[ServiceRequest], bytes]
AnyParameterValue: TypeAlias = Union[
    ParameterValue.Integer,
    ParameterValue.Bool,
    ParameterValue.Float64,
    ParameterValue.String,
    ParameterValue.Array,
    ParameterValue.Dict,
]
AnyInnerParameterValue: TypeAlias = Union[
    AnyParameterValue,
    bool,
    int,
    float,
    str,
    "list[AnyInnerParameterValue]",
    "dict[str, AnyInnerParameterValue]",
]
AnyNativeParameterValue: TypeAlias = Union[
    AnyInnerParameterValue,
    bytes,
]
AssetHandler: TypeAlias = "Callable[[str], bytes | None]"

# Deprecated. Use foxglove.mcap.MCAPWriter instead.
from .mcap import MCAPWriter

if TYPE_CHECKING:
    from .notebook.notebook_buffer import NotebookBuffer

atexit.register(_foxglove.shutdown)


try:
    from .websocket import Capability as WebSocketCapability
    from .websocket import (
        ServerListener,
        WebSocketServer,
    )

    def start_server(
        *,
        name: str | None = None,
        host: str | None = "127.0.0.1",
        port: int | None = 8765,
        capabilities: list[WebSocketCapability] | None = None,
        server_listener: ServerListener | None = None,
        supported_encodings: list[str] | None = None,
        services: list[Service] | None = None,
        asset_handler: AssetHandler | None = None,
        context: Context | None = None,
        session_id: str | None = None,
        channel_filter: SinkChannelFilter | None = None,
        playback_time_range: tuple[int, int] | None = None,
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
        :param channel_filter: A ``Callable`` that determines whether a channel should be logged to.
            Return ``True`` to log the channel, or ``False`` to skip it. By default, all channels
            will be logged.
        :param playback_time_range: Time range of data being played back, in absolute nanoseconds.
            Implies ``Capability.PlaybackControl`` if set.
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
            channel_filter=channel_filter,
            playback_time_range=playback_time_range,
        )

    __all__ += ["start_server"]

except ImportError:
    pass


try:
    from .remote_access import Capability as RemoteAccessCapability
    from .remote_access import (
        RemoteAccessGateway,
        RemoteAccessListener,
    )

    def start_gateway(
        *,
        name: str | None = None,
        device_token: str | None = None,
        capabilities: list[RemoteAccessCapability] | None = None,
        listener: RemoteAccessListener | None = None,
        supported_encodings: list[str] | None = None,
        services: list[Service] | None = None,
        context: Context | None = None,
        channel_filter: SinkChannelFilter | None = None,
        message_backlog_size: int | None = None,
        foxglove_api_url: str | None = None,
        foxglove_api_timeout: float | None = None,
    ) -> RemoteAccessGateway:
        """
        Start a remote access gateway for live visualization and teleop in Foxglove.

        :param name: The name of the server. If not set, the device name from the Foxglove
            platform is used.
        :param device_token: The device token for authenticating with the Foxglove platform.
            If not set, the ``FOXGLOVE_DEVICE_TOKEN`` environment variable is used.
        :param capabilities: A list of capabilities to advertise to clients.
        :param listener: A Python object that implements the
            :py:class:`foxglove.remote_access.RemoteAccessListener` protocol.
        :param supported_encodings: A list of encodings to advertise to clients.
        :param services: A list of services to advertise to clients.
        :param context: The context to use for logging. If None, the global context is used.
        :param channel_filter: A ``Callable`` that determines whether a channel should be logged
            to. Return ``True`` to log the channel, or ``False`` to skip it. By default, all
            channels will be logged.
        :param message_backlog_size: The maximum number of messages to buffer before dropping
            the oldest entries. Defaults to 1024.
        :param foxglove_api_url: Override the Foxglove API base URL.
        :param foxglove_api_timeout: Timeout for Foxglove API requests, in seconds.
        """
        return _foxglove.start_gateway(
            name=name,
            device_token=device_token,
            capabilities=capabilities,
            listener=listener,
            supported_encodings=supported_encodings,
            services=services,
            context=context,
            channel_filter=channel_filter,
            message_backlog_size=message_backlog_size,
            foxglove_api_url=foxglove_api_url,
            foxglove_api_timeout=foxglove_api_timeout,
        )

    __all__ += ["start_gateway"]

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


def init_notebook_buffer(context: Context | None = None) -> NotebookBuffer:
    """
    Create a NotebookBuffer object to manage data buffering and visualization in Jupyter notebooks.

    The NotebookBuffer object will buffer all data logged to the provided context. When you are
    ready to visualize the data, you can call the
    :meth:`~notebook.notebook_buffer.NotebookBuffer.show` method to display an embedded Foxglove
    visualization widget. The widget provides a fully-featured Foxglove interface directly within
    your Jupyter notebook, allowing you to explore multi-modal robotics data including 3D scenes,
    plots, images, and more.

    :param context: The Context used to log the messages. If no Context is provided, the global
        context will be used. Logged messages will be buffered.

    :returns: A NotebookBuffer object that can be used to manage the data buffering and
        visualization.

    :raises Exception: If the notebook extra package is not installed. Install it with ``pip install
        foxglove-sdk[notebook]``.

    :note: This function is only available when the ``notebook`` extra package is installed. Install
        it with ``pip install foxglove-sdk[notebook]``.

    Example:

    .. code-block:: python

        import foxglove

        # Create a basic viewer using the default context
        nb_buffer = foxglove.init_notebook_buffer()

        # Or use a specific context
        nb_buffer = foxglove.init_notebook_buffer(context=my_ctx)

        # ... log data as usual ...

        # Display the widget in the notebook
        nb_buffer.show()
    """
    try:
        from .notebook.notebook_buffer import NotebookBuffer

    except ImportError:
        raise Exception(
            "NotebookBuffer is not installed. "
            'Please install it with `pip install "foxglove-sdk[notebook]"`'
        )

    return NotebookBuffer(context=context)


__all__ = [
    "AnyInnerParameterValue",
    "AnyNativeParameterValue",
    "AnyParameterValue",
    "AssetHandler",
    "Channel",
    "ChannelDescriptor",
    "ConnectionGraph",
    "Context",
    "MCAPWriter",
    "MessageSchema",
    "Parameter",
    "ParameterType",
    "ParameterValue",
    "Schema",
    "Service",
    "ServiceHandler",
    "ServiceRequest",
    "ServiceSchema",
    "SinkChannelFilter",
    "log",
    "open_mcap",
    "set_log_level",
    "init_notebook_buffer",
]
