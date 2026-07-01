from enum import Enum

from foxglove import (
    ChannelDescriptor,
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

class Reliability(Enum):
    """
    The reliability policy for a channel's data delivery.
    """

    Lossy = ...
    """Data is sent over unreliable data tracks. This is the default."""

    Reliable = ...
    """Data is sent over the reliable control channel (ordered, guaranteed delivery)."""

class QosProfile:
    """
    Quality-of-service profile for a channel.
    """

    reliability: Reliability

    def __init__(self, *, reliability: Reliability = Reliability.Lossy) -> None: ...

class Capability(Enum):
    """
    An enumeration of capabilities that the remote access gateway can advertise to its clients.
    """

    ClientPublish = ...
    """Allow clients to advertise channels to send data messages to the server."""

    ConnectionGraph = ...
    """Allow clients to subscribe to connection graph updates."""

    Parameters = ...
    """Allow clients to get, set, and subscribe to parameter updates."""

    Services = ...
    """Allow clients to call services."""

class VideoEncoderBackend(Enum):
    """
    The preferred backend for encoding published video tracks.

    This is a gateway-wide preference applied to every published video track. If the requested
    backend is unavailable on the host, the SDK falls back to another compatible encoder.
    """

    Auto = ...
    """Let the SDK choose the encoder backend (honoring ``FOXGLOVE_VIDEO_ENCODER``). The default."""

    Software = ...
    """Prefer a software encoder."""

    Hardware = ...
    """Prefer any available hardware encoder."""

    Nvenc = ...
    """Prefer NVIDIA NVENC when available."""

    Vaapi = ...
    """Prefer VAAPI when available."""

    VideoToolbox = ...
    """Prefer VideoToolbox on Apple platforms when available."""

class Client:
    """
    A client connected to a running remote access gateway.
    """

    id: int = ...

class RemoteAccessConnectionStatus(Enum):
    """
    The status of the remote access gateway connection.
    """

    Connecting = ...
    """The gateway is attempting to establish or re-establish a connection."""

    Connected = ...
    """The gateway is connected and handling events."""

    ShuttingDown = ...
    """The gateway is shutting down. Listener callbacks may still be in progress."""

    Shutdown = ...
    """The gateway has been shut down. No further listener callbacks will be invoked."""

class RemoteAccessGateway:
    """
    A running remote access gateway.
    """

    def connection_status(self) -> RemoteAccessConnectionStatus:
        """Returns the current connection status."""
        ...

    def add_services(self, services: list[Service]) -> None:
        """Advertises support for the provided services."""
        ...

    def remove_services(self, names: list[str]) -> None:
        """Removes services that were previously advertised."""
        ...

    def publish_parameter_values(self, parameters: list[Parameter]) -> None:
        """Publishes parameter values to all subscribed clients."""
        ...

    def publish_status(
        self, message: str, level: StatusLevel, id: str | None = None
    ) -> None:
        """Publishes a status message to all connected participants."""
        ...

    def remove_status(self, ids: list[str]) -> None:
        """Removes status messages by ID from all connected participants."""
        ...

    def publish_connection_graph(self, graph: ConnectionGraph) -> None:
        """
        Publishes a connection graph update to all subscribed clients. An update is published to
        clients as a difference from the current graph to the replacement graph. When a client first
        subscribes to connection graph updates, it receives the current graph.

        Raises an error if the gateway wasn't started with :py:attr:`Capability.ConnectionGraph`.
        """
        ...

    def stop(self) -> None:
        """Gracefully disconnect from the remote access gateway."""
        ...
