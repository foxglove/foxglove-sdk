from collections.abc import Callable
from enum import Enum
from typing import Dict, List, Optional, Union

import foxglove

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
    def set_published_topic(self, topic: str, publisher_ids: List[str]) -> None:
        """
        Set a published topic and its associated publisher ids. Overwrites any existing topic with
        the same name.

        :param topic: The topic name.
        :param publisher_ids: The set of publisher ids.
        """
        ...

    def set_subscribed_topic(self, topic: str, subscriber_ids: List[str]) -> None:
        """
        Set a subscribed topic and its associated subscriber ids. Overwrites any existing topic with
        the same name.

        :param topic: The topic name.
        :param subscriber_ids: The set of subscriber ids.
        """
        ...

    def set_advertised_service(self, service: str, provider_ids: List[str]) -> None:
        """
        Set an advertised service and its associated provider ids Overwrites any existing service
        with the same name.

        :param service: The service name.
        :param provider_ids: The set of provider ids.
        """
        ...

class MessageSchema:
    """
    A service request or response schema.
    """

    encoding: str
    schema: "foxglove.Schema"

    def __new__(
        cls,
        *,
        encoding: str,
        schema: "foxglove.Schema",
    ) -> "MessageSchema": ...

class Parameter:
    """
    A parameter which can be sent to a client.

    :param name: The parameter name.
    :type name: str
    :param value: Optional value, represented as a native python object, or a ParameterValue.
    :type value: None|bool|float|str|bytes|list|dict|ParameterValue
    :param type: Optional parameter type. This is automatically derived when passing a native
                 python object as the value.
    :type type: ParameterType|None
    """

    name: str
    type: Optional["ParameterType"]
    value: Optional["AnyParameterValue"]

    def __init__(
        self,
        name: str,
        *,
        value: Optional["AnyNativeParameterValue"] = None,
        type: Optional["ParameterType"] = None,
    ) -> None: ...
    def get_value(self) -> Optional["AnyNativeParameterValue"]:
        """Returns the parameter value as a native python object."""
        ...

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
    A parameter value.
    """

    class Bool:
        """A boolean value."""

        def __new__(cls, value: bool) -> "ParameterValue.Bool": ...

    class Number:
        """A decimal or integer value."""

        def __new__(cls, value: float) -> "ParameterValue.Number": ...

    class String:
        """
        A string value.

        For parameters of type :py:attr:ParameterType.ByteArray, this is a
        base64 encoding of the byte array.
        """

        def __new__(cls, value: str) -> "ParameterValue.String": ...

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
    ParameterValue.String,
    ParameterValue.Array,
    ParameterValue.Dict,
]

AnyInnerParameterValue = Union[
    AnyParameterValue,
    bool,
    float,
    str,
    List["AnyInnerParameterValue"],
    Dict[str, "AnyInnerParameterValue"],
]

AnyNativeParameterValue = Union[
    AnyInnerParameterValue,
    bytes,
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
    """A level for `WebSocketServer.publish_status`"""

    Info = ...
    Warning = ...
    Error = ...

class WebSocketServer:
    """
    A websocket server for live visualization.
    """

    def __new__(cls) -> "WebSocketServer": ...
    @property
    def port(self) -> int:
        """Get the port on which the server is listening."""
        ...

    def app_url(
        self,
        *,
        layout_id: Optional[str] = None,
        open_in_desktop: bool = False,
    ) -> Optional[str]:
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

    def clear_session(self, session_id: Optional[str] = None) -> None:
        """
        Sets a new session ID and notifies all clients, causing them to reset their state.
        If no session ID is provided, generates a new one based on the current timestamp.
        If the server has been stopped, this has no effect.
        """
        ...

    def broadcast_time(self, timestamp_nanos: int) -> None:
        """
        Publishes the current server timestamp to all clients.
        If the server has been stopped, this has no effect.
        """
        ...

    def publish_parameter_values(self, parameters: List["Parameter"]) -> None:
        """Publishes parameter values to all subscribed clients."""
        ...

    def publish_status(
        self, message: str, level: "StatusLevel", id: Optional[str] = None
    ) -> None:
        """
        Send a status message to all clients. If the server has been stopped, this has no effect.
        """
        ...

    def remove_status(self, ids: list[str]) -> None:
        """
        Remove status messages by id from all clients. If the server has been stopped, this has no
        effect.
        """
        ...

    def add_services(self, services: list["Service"]) -> None:
        """Add services to the server."""
        ...

    def remove_services(self, names: list[str]) -> None:
        """Removes services that were previously advertised."""
        ...

    def publish_connection_graph(self, graph: "ConnectionGraph") -> None:
        """
        Publishes a connection graph update to all subscribed clients. An update is published to
        clients as a difference from the current graph to the replacement graph. When a client first
        subscribes to connection graph updates, it receives the current graph.
        """
        ...
