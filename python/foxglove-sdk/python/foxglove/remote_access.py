from __future__ import annotations

from typing import Protocol

from foxglove import (
    ChannelDescriptor,
    MessageSchema,
    Parameter,
    ParameterType,
    ParameterValue,
    Service,
    ServiceRequest,
    ServiceSchema,
)

from ._foxglove_py.remote_access import (
    Capability,
    Client,
    RemoteAccessConnectionStatus,
    RemoteAccessGateway,
)


class RemoteAccessListener(Protocol):
    """
    A mechanism to register callbacks for handling remote access client events.
    """

    def on_connection_status_changed(
        self, status: RemoteAccessConnectionStatus
    ) -> None:
        """
        Called when the gateway connection status changes.

        :param status: The new connection status.
        """
        return None

    def on_subscribe(self, client: Client, channel: ChannelDescriptor) -> None:
        """
        Called when a client subscribes to a channel.

        :param client: The client that subscribed.
        :param channel: The channel that was subscribed to.
        """
        return None

    def on_unsubscribe(self, client: Client, channel: ChannelDescriptor) -> None:
        """
        Called when a client unsubscribes from a channel or disconnects.
        Also called when a subscribed channel is removed from the :class:`~foxglove.Context`.

        :param client: The client that unsubscribed.
        :param channel: The channel that was unsubscribed from.
        """
        return None

    def on_client_advertise(self, client: Client, channel: ChannelDescriptor) -> None:
        """
        Called when a client advertises a channel.

        :param client: The client that advertised the channel.
        :param channel: The channel that was advertised.
        """
        return None

    def on_client_unadvertise(self, client: Client, channel: ChannelDescriptor) -> None:
        """
        Called when a client unadvertises a channel.

        :param client: The client that unadvertised the channel.
        :param channel: The channel that was unadvertised.
        """
        return None

    def on_message_data(
        self, client: Client, channel: ChannelDescriptor, data: bytes
    ) -> None:
        """
        Called when a message is received from a client.

        :param client: The client that sent the message.
        :param channel: The channel the message was sent on.
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
        Called when a client requests parameters.

        Requires :py:data:`Capability.Parameters`.

        :param client: The client that sent the request.
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
        Called when a client sets parameters.

        Requires :py:data:`Capability.Parameters`.

        :param client: The client that sent the request.
        :param parameters: The parameters to set.
        :param request_id: An optional request id.
        """
        return parameters

    def on_parameters_subscribe(self, param_names: list[str]) -> None:
        """
        Called when a client subscribes to one or more parameters for the first time.

        Requires :py:data:`Capability.Parameters`.

        :param param_names: The names of the parameters to subscribe to.
        """
        return None

    def on_parameters_unsubscribe(self, param_names: list[str]) -> None:
        """
        Called when the last client subscription to one or more parameters has been removed.

        Requires :py:data:`Capability.Parameters`.

        :param param_names: The names of the parameters to unsubscribe from.
        """
        return None


__all__ = [
    "Capability",
    "Client",
    "MessageSchema",
    "Parameter",
    "ParameterType",
    "ParameterValue",
    "RemoteAccessConnectionStatus",
    "RemoteAccessGateway",
    "RemoteAccessListener",
    "Service",
    "ServiceRequest",
    "ServiceSchema",
]
