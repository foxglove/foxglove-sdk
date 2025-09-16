from __future__ import annotations

import sys
from collections.abc import Callable
from typing import Protocol, Union

if sys.version_info >= (3, 10):
    from typing import TypeAlias
else:
    from typing import Any as TypeAlias

from ._foxglove_py.websocket import (
    Capability,
    ChannelView,
    Client,
    ClientChannel,
    ConnectionGraph,
    MessageSchema,
    Parameter,
    ParameterType,
    ParameterValue,
    Service,
    ServiceRequest,
    ServiceSchema,
    StatusLevel,
    WebSocketServer,
)

ServiceHandler: TypeAlias = Callable[[ServiceRequest], bytes]
AssetHandler: TypeAlias = Callable[[str], "bytes | None"]
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


class ServerListener(Protocol):
    """
    A mechanism to register callbacks for handling client message events.
    """

    def on_subscribe(self, client: Client, channel: ChannelView) -> None:
        """
        Called by the server when a client subscribes to a channel.

        :param client: The client (id) that sent the message.
        :param channel: The channel (id, topic) that the message was sent on.
        """
        return None

    def on_unsubscribe(self, client: Client, channel: ChannelView) -> None:
        """
        Called by the server when a client unsubscribes from a channel or disconnects.

        :param client: The client (id) that sent the message.
        :param channel: The channel (id, topic) that the message was sent on.
        """
        return None

    def on_client_advertise(self, client: Client, channel: ClientChannel) -> None:
        """
        Called by the server when a client advertises a channel.

        :param client: The client (id) that sent the message.
        :param channel: The client channel that is being advertised.
        """
        return None

    def on_client_unadvertise(self, client: Client, client_channel_id: int) -> None:
        """
        Called by the server when a client unadvertises a channel.

        :param client: The client (id) that is unadvertising the channel.
        :param client_channel_id: The client channel id that is being unadvertised.
        """
        return None

    def on_message_data(
        self, client: Client, client_channel_id: int, data: bytes
    ) -> None:
        """
        Called by the server when a message is received from a client.

        :param client: The client (id) that sent the message.
        :param client_channel_id: The client channel id that the message was sent on.
        :param data: The message data.
        """
        return None

    def on_get_parameters(
        self,
        client: Client,
        param_names: list[str],
        request_id: str | None = None,
    ) -> list[Parameter]:
        """
        Called by the server when a client requests parameters.

        Requires :py:data:`Capability.Parameters`.

        :param client: The client (id) that sent the message.
        :param param_names: The names of the parameters to get.
        :param request_id: An optional request id.
        """
        return []

    def on_set_parameters(
        self,
        client: Client,
        parameters: list[Parameter],
        request_id: str | None = None,
    ) -> list[Parameter]:
        """
        Called by the server when a client sets parameters.
        Note that only `parameters` which have changed are included in the callback, but the return
        value must include all parameters. If a parameter that is unset is included in the return
        value, it will not be published to clients.

        Requires :py:data:`Capability.Parameters`.

        :param client: The client (id) that sent the message.
        :param parameters: The parameters to set.
        :param request_id: An optional request id.
        """
        return parameters

    def on_parameters_subscribe(
        self,
        param_names: list[str],
    ) -> None:
        """
        Called by the server when a client subscribes to one or more parameters for the first time.

        Requires :py:data:`Capability.Parameters`.

        :param param_names: The names of the parameters to subscribe to.
        """
        return None

    def on_parameters_unsubscribe(
        self,
        param_names: list[str],
    ) -> None:
        """
        Called by the server when the last client subscription to one or more parameters has been
        removed.

        Requires :py:data:`Capability.Parameters`.

        :param param_names: The names of the parameters to unsubscribe from.
        """
        return None

    def on_connection_graph_subscribe(self) -> None:
        """
        Called by the server when the first client subscribes to the connection graph.
        """
        return None

    def on_connection_graph_unsubscribe(self) -> None:
        """
        Called by the server when the last client unsubscribes from the connection graph.
        """
        return None


__all__ = [
    "AnyInnerParameterValue",
    "AnyNativeParameterValue",
    "AnyParameterValue",
    "AssetHandler",
    "Capability",
    "ChannelView",
    "Client",
    "ClientChannel",
    "ConnectionGraph",
    "MessageSchema",
    "Parameter",
    "ParameterType",
    "ParameterValue",
    "ServerListener",
    "Service",
    "ServiceHandler",
    "ServiceRequest",
    "ServiceSchema",
    "StatusLevel",
    "WebSocketServer",
]
