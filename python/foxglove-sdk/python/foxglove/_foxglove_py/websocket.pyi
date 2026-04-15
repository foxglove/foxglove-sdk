from enum import Enum

from foxglove import (
    AnyNativeParameterValue,
    AnyParameterValue,
    ConnectionGraph,
    MessageSchema,
    Parameter,
    ParameterType,
    ParameterValue,
    Service,
    ServiceHandler,
    ServiceRequest,
    ServiceSchema,
    StatusLevel,
)

class Capability(Enum):
    """
    An enumeration of capabilities that the websocket server can advertise to its clients.
    """

    ClientPublish = ...
    """Allow clients to advertise channels to send data messages to the server."""

    ConnectionGraph = ...
    """Allow clients to subscribe and make connection graph updates"""

    Parameters = ...
    """Allow clients to get & set parameters."""

    Services = ...
    """Allow clients to call services."""

    Time = ...
    """Inform clients about the latest server time."""

    PlaybackControl = ...
    """Indicates that the server is capable of responding to playback control requests from controls in the Foxglove app."""

class Client:
    """
    A client that is connected to a running websocket server.
    """

    id: int = ...

class ChannelView:
    """
    Information about a channel.
    """

    id: int = ...
    topic: str = ...

class ClientChannel:
    """
    Information about a channel advertised by a client.
    """

    id: int = ...
    topic: str = ...
    encoding: str = ...
    schema_name: str = ...
    schema_encoding: str | None = ...
    schema: bytes | None = ...

class PlaybackCommand(Enum):
    """The command for playback requested by the client player"""

    Play = ...
    Pause = ...

class PlaybackControlRequest:
    """
    A request to control playback from the client

    :param playback_command: The command for playback requested by the client player
    :type playback_command: PlaybackCommand
    :param playback_speed: The speed of playback requested by the client player
    :type playback_speed: float
    :param seek_time: The time the client player is requesting to seek to, in nanoseconds. None if no seek is requested.
    :type seek_time: int | None
    :param request_id: Unique string identifier, used to indicate that a PlaybackState is in response to a particular request from the client.
    :type request_id: str
    """

    playback_command: PlaybackCommand
    playback_speed: float
    seek_time: int | None
    request_id: str

class PlaybackState:
    """
    The state of data playback on the server

    :param status: The status of server data playback
    :type status: PlaybackStatus
    :param current_time: The current time of playback, in absolute nanoseconds
    :type current_time: int
    :param playback_speed: The speed of playback, as a factor of realtime
    :type playback_speed: float
    :param did_seek: Whether a seek forward or backward in time triggered this message to be emitted
    :type did_seek: bool
    :param request_id: If this message is being emitted in response to a PlaybackControlRequest message, the request_id from that message. Set this to an empty string if the state of playback has been changed by any other condition.
    :type request_id: str | None
    """

    status: PlaybackStatus
    current_time: int
    playback_speed: float
    did_seek: bool
    request_id: str | None

    def __init__(
        self,
        status: PlaybackStatus,
        current_time: int,
        playback_speed: float,
        did_seek: bool,
        request_id: str | None,
    ): ...

class PlaybackStatus(Enum):
    """The status of server data playback"""

    Playing = ...
    Paused = ...
    Buffering = ...
    Ended = ...

class WebSocketServer:
    """
    A websocket server for live visualization.
    """

    def __init__(self) -> None: ...
    @property
    def port(self) -> int:
        """Get the port on which the server is listening."""
        ...

    def app_url(
        self,
        *,
        layout_id: str | None = None,
        open_in_desktop: bool = False,
    ) -> str | None:
        """
        Returns a web app URL to open the websocket as a data source.

        Returns None if the server has been stopped.

        :param layout_id: An optional layout ID to include in the URL.
        :param open_in_desktop: Opens the foxglove desktop app.
        """
        ...

    def stop(self) -> None:
        """Explicitly stop the server."""
        ...

    def clear_session(self, session_id: str | None = None) -> None:
        """
        Sets a new session ID and notifies all clients, causing them to reset their state.
        If no session ID is provided, generates a new one based on the current timestamp.
        If the server has been stopped, this has no effect.
        """
        ...

    def broadcast_playback_state(self, playback_state: PlaybackState) -> None:
        """
        Publish the current playback state to all clients.
        """
        ...

    def broadcast_time(self, timestamp_nanos: int) -> None:
        """
        Publishes the current server timestamp to all clients.
        If the server has been stopped, this has no effect.
        """
        ...

    def publish_parameter_values(self, parameters: list[Parameter]) -> None:
        """Publishes parameter values to all subscribed clients."""
        ...

    def publish_status(
        self, message: str, level: StatusLevel, id: str | None = None
    ) -> None:
        """
        Send a status message to all clients. If the server has been stopped, this has no effect.
        """
        ...

    def remove_status(self, ids: list[str]) -> None:
        """
        Remove status messages by ID from all clients. If the server has been stopped, this has no
        effect.
        """
        ...

    def add_services(self, services: list[Service]) -> None:
        """
        Add services to the server.

        This method will fail if the server was not configured with
        :py:attr:`Capability.Services`, if a service name is not unique, or if a service has no
        request encoding and the server has no supported encodings.
        """
        ...

    def remove_services(self, names: list[str]) -> None:
        """Removes services that were previously advertised."""
        ...

    def publish_connection_graph(self, graph: ConnectionGraph) -> None:
        """
        Publishes a connection graph update to all subscribed clients. An update is published to
        clients as a difference from the current graph to the replacement graph. When a client first
        subscribes to connection graph updates, it receives the current graph.
        """
        ...
