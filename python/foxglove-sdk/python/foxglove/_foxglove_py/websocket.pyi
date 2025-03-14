from collections.abc import Callable
from enum import Enum
from typing import List, Optional, Union

from . import Schema

class Capability(Enum):
    """
    An enumeration of capabilities that the websocket server can advertise to its clients.
    """

    ClientPublish = ...
    """Allow clients to advertise channels to send data messages to the server."""

    Connectiongraph = ...
    """Allow clients to subscribe and make connection graph updates"""

    Parameters = ...
    """Allow clients to get & set parameters."""

    Services = ...
    """Allow clients to call services."""

    Time = ...
    """Inform clients about the latest server time."""

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
    schema_encoding: Optional[str] = ...
    schema: Optional[bytes] = ...

class ConnectionGraph:
    """
    A graph of connections between clients.
    """

    def __new__(cls) -> "ConnectionGraph": ...
    def set_published_topic(self, topic: str, publisher_ids: List[str]) -> None: ...
    def set_subscribed_topic(self, topic: str, subscriber_ids: List[str]) -> None: ...
    def set_advertised_service(self, service: str, provider_ids: List[str]) -> None: ...

class MessageSchema:
    """
    A service request or response schema.
    """

    encoding: str
    schema: "Schema"

    def __new__(
        cls,
        *,
        encoding: str,
        schema: "Schema",
    ) -> "MessageSchema": ...

class Parameter:
    """
    A parameter.
    """

    name: str
    type: Optional["ParameterType"]
    value: Optional["AnyParameterValue"]

    def __init__(
        self,
        name: str,
        *,
        type: Optional["ParameterType"] = None,
        value: Optional["AnyParameterValue"] = None,
    ) -> None: ...

class ParameterType(Enum):
    """
    The type of a parameter.
    """

    ByteArray = ...
    """A byte array."""

    Float64 = ...
    """A decimal or integer value that can be represented as a `float64`."""

    Float64Array = ...
    """An array of decimal or integer values that can be represented as `float64`s."""

class ParameterValue:
    """
    The value of a parameter.
    """

    class Bool:
        """A boolean value."""

        def __new__(cls, value: bool) -> "ParameterValue.Bool": ...

    class Number:
        """A decimal or integer value."""

        def __new__(cls, value: float) -> "ParameterValue.Number": ...

    class Bytes:
        """A byte array."""

        def __new__(cls, value: bytes) -> "ParameterValue.Bytes": ...

    class Array:
        """An array of parameter values."""

        def __new__(
            cls, value: List["AnyParameterValue"]
        ) -> "ParameterValue.Array": ...

    class Dict:
        """An associative map of parameter values."""

        def __new__(
            cls, value: dict[str, "AnyParameterValue"]
        ) -> "ParameterValue.Dict": ...

AnyParameterValue = Union[
    ParameterValue.Bool,
    ParameterValue.Number,
    ParameterValue.Bytes,
    ParameterValue.Array,
    ParameterValue.Dict,
]

AssetHandler = Callable[[str], Optional[bytes]]

class ServiceRequest:
    """
    A websocket service request.
    """

    service_name: str
    client_id: int
    call_id: int
    encoding: str
    payload: bytes

ServiceHandler = Callable[["ServiceRequest"], bytes]

class Service:
    """
    A websocket service.
    """

    name: str
    schema: "ServiceSchema"
    handler: "ServiceHandler"

    def __new__(
        cls,
        *,
        name: str,
        schema: "ServiceSchema",
        handler: "ServiceHandler",
    ) -> "Service": ...

class ServiceSchema:
    """
    A websocket service schema.
    """

    name: str
    request: Optional["MessageSchema"]
    response: Optional["MessageSchema"]

    def __new__(
        cls,
        *,
        name: str,
        request: Optional["MessageSchema"] = None,
        response: Optional["MessageSchema"] = None,
    ) -> "ServiceSchema": ...

class StatusLevel(Enum):
    Info = ...
    Warning = ...
    Error = ...

class WebSocketServer:
    """
    A websocket server for live visualization.
    """

    def __new__(cls) -> "WebSocketServer": ...
    @property
    def port(self) -> int: ...
    def stop(self) -> None: ...
    def clear_session(self, session_id: Optional[str] = None) -> None: ...
    def broadcast_time(self, timestamp_nanos: int) -> None: ...
    def publish_parameter_values(self, parameters: List["Parameter"]) -> None: ...
    def publish_status(
        self, message: str, level: "StatusLevel", id: Optional[str] = None
    ) -> None: ...
    def remove_status(self, ids: list[str]) -> None: ...
    def add_services(self, services: list["Service"]) -> None: ...
    def remove_services(self, names: list[str]) -> None: ...
    def publish_connection_graph(self, graph: "ConnectionGraph") -> None: ...
