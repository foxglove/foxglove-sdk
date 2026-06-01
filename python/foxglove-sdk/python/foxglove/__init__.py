"""
This module provides interfaces for logging messages to Foxglove.

See :py:mod:`foxglove.messages` and :py:mod:`foxglove.channels` for working with well-known Foxglove
message types.
"""

from __future__ import annotations

import atexit
import logging
import sys
from collections.abc import Callable
from typing import TYPE_CHECKING, TypeAlias, Union

from . import _foxglove_py as _foxglove

# Re-export these imports
from ._foxglove_py import (
    ChannelDescriptor,
    Context,
    Schema,
    SinkChannelFilter,
    open_mcap,
)
from .channel import Channel, log

# Deprecated. Use foxglove.mcap.MCAPWriter instead.
from .mcap import MCAPWriter

if TYPE_CHECKING:
    from .notebook.notebook_buffer import NotebookBuffer

atexit.register(_foxglove.shutdown)

__all__ = [
    "Channel",
    "ChannelDescriptor",
    "Context",
    "MCAPWriter",
    "Schema",
    "SinkChannelFilter",
    "log",
    "open_mcap",
    "set_log_level",
    "init_notebook_buffer",
]

# Re-export these imports (not available in WASM)
try:
    from ._foxglove_py import (  # noqa: F401
        ConnectionGraph,
        MessageSchema,
        Parameter,
        ParameterType,
        ParameterValue,
        Service,
        ServiceRequest,
        ServiceSchema,
        StatusLevel,
    )

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

    __all__.extend(
        [
            "AnyInnerParameterValue",
            "AnyNativeParameterValue",
            "AnyParameterValue",
            "AssetHandler",
            "ConnectionGraph",
            "MessageSchema",
            "Parameter",
            "ParameterType",
            "ParameterValue",
            "Service",
            "ServiceHandler",
            "ServiceRequest",
            "ServiceSchema",
            "StatusLevel",
        ]
    )
except ImportError:
    if sys.platform != "emscripten":
        raise


try:
    from .websocket import (
        Capability,
        ServerListener,
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
        channel_filter: SinkChannelFilter | None = None,
        playback_time_range: tuple[int, int] | None = None,
        message_backlog_size: int | None = None,
    ) -> WebSocketServer:
        """
        Start a WebSocket server for live visualization.

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
        :param message_backlog_size: The maximum number of outgoing messages to buffer per client.
            The oldest data-plane message is dropped when the buffer fills. The control-plane queue
            is the same size; if it fills, the slow client is disconnected. Defaults to 1024.
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
            message_backlog_size=message_backlog_size,
        )

    __all__ += [
        "Capability",  # for backwards compatibility
        "start_server",
    ]

except ImportError:
    if sys.platform != "emscripten":
        raise


try:
    from ._foxglove_py import SystemInfoPublisher
    from ._foxglove_py import start_sysinfo_publisher as _start_sysinfo_publisher

    # Keep this doc string in sync with rust/foxglove/src/system_info.rs
    def start_sysinfo_publisher(
        *,
        topic: str | None = None,
        refresh_interval: float | None = None,
        context: Context | None = None,
    ) -> SystemInfoPublisher:
        """
        Start the system info publisher.

        Periodically publishes a ``SystemInfo`` message to a channel containing process and
        system statistics (memory, CPU, OS info).

        .. rubric:: Published metrics

        Each message is a JSON object with a JSON Schema attached to the channel.
        The following fields are published:

        - ``process_memory`` (number): Resident memory used by the SDK process, in bytes.
        - ``process_virtual_memory`` (number): Virtual memory used by the SDK process, in bytes.
        - ``process_cpu_percent`` (number): CPU usage for the SDK process, as a percent of total
          system CPU capacity (0.0 to 100.0).
        - ``process_cpu_cores`` (number): CPU usage for the SDK process, expressed in
          core-equivalents (0.0 to ``num_cpus``). 1.0 means a single logical CPU is fully utilized.
        - ``total_cpu_percent`` (number): Total CPU usage across all logical CPUs on the system,
          as a percent (0.0 to 100.0).
        - ``total_cpu_cores`` (number): Total CPU usage across the system, expressed in
          core-equivalents (0.0 to ``num_cpus``). 1.0 means one logical CPU's worth of work is
          being done.
        - ``num_cpus`` (integer): Number of logical CPUs on the system.
        - ``total_memory`` (number): Total physical memory on the system, in bytes.
        - ``used_memory`` (number): Used physical memory on the system, in bytes.
        - ``total_swap`` (number): Total swap space on the system, in bytes.
        - ``used_swap`` (number): Used swap space on the system, in bytes.
        - ``kernel_version`` (string): Kernel version string, or empty if unknown.
        - ``os_version`` (string): OS version string, or empty if unknown.

        CPU usage values are computed from the difference between consecutive samples, so they
        reflect activity over the most recent refresh interval.

        The caller is responsible for calling stop() on the returned handle when done;
        dropping the handle does not stop the background task.

        :param topic: The channel topic name. Defaults to ``/sysinfo``.
        :param refresh_interval: How often to publish, in seconds. Defaults to ``0.5``.
            Clamped to a minimum of 0.2s.
        :param context: The context on which the publisher creates its channel. Defaults to
            the global default context.
        :returns: A handle that can be used to stop the publisher.
        """
        return _start_sysinfo_publisher(
            topic=topic,
            refresh_interval=refresh_interval,
            context=context,
        )

    __all__ += [
        "SystemInfoPublisher",
        "start_sysinfo_publisher",
    ]

except ImportError:
    if sys.platform != "emscripten":
        raise


try:
    from .remote_access import Capability as RemoteAccessCapability
    from .remote_access import (
        QosProfile,
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
        qos_classifier: Callable[[ChannelDescriptor], QosProfile] | None = None,
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
        :param qos_classifier: A ``Callable`` that returns the
            :py:class:`foxglove.remote_access.QosProfile` to use for a given channel. If not set,
            all channels use the default lossy profile.
        :param message_backlog_size: The maximum number of messages to buffer before disconnecting
            the slow client. Defaults to 1024.
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
            qos_classifier=qos_classifier,
            message_backlog_size=message_backlog_size,
            foxglove_api_url=foxglove_api_url,
            foxglove_api_timeout=foxglove_api_timeout,
        )

    __all__ += ["start_gateway"]

except ImportError:
    # Remote access is only included on supported platforms.
    pass


def set_log_level(level: int | str = "INFO") -> None:
    """
    Sets the global log level for the Foxglove SDK and initializes logging.

    If FOXGLOVE_LOG_LEVEL is set, that's used instead of the passed level.

    This function should be called before other Foxglove initialization to capture output from all
    components. Calling this after starting a sink (e.g. server, gateway, or mcap) will
    have no effect. Only the first call to this function will have an effect. Thread-safe.

    This calls logging.basicConfig to setup a global logger if one is not already configured.
    Set up your logging before calling this function to avoid that.

    :param level: The logging level to set. This accepts the same values as `logging.setLevel` and
        defaults to "INFO". The SDK will not log at levels "CRITICAL" or higher.
    """

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
