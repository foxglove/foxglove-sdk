from enum import Enum

from .websocket import MessageSchema as MessageSchema
from .websocket import Service as Service
from .websocket import ServiceRequest as ServiceRequest
from .websocket import ServiceSchema as ServiceSchema

class Capability(Enum):
    """
    An enumeration of capabilities that the remote access gateway can advertise to its clients.
    """

    ClientPublish = ...
    """Allow clients to advertise channels to send data messages to the server."""

    Services = ...
    """Allow clients to call services."""

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

    def stop(self) -> None:
        """Gracefully disconnect from the remote access gateway."""
        ...
