from __future__ import annotations

from typing import Protocol

from foxglove import ChannelDescriptor

from ._foxglove_py.remote_access import (
    Capability,
    Client,
    MessageSchema,
    RemoteAccessConnectionStatus,
    RemoteAccessGateway,
    Service,
    ServiceRequest,
    ServiceSchema,
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


__all__ = [
    "Capability",
    "Client",
    "RemoteAccessConnectionStatus",
    "RemoteAccessGateway",
    "RemoteAccessListener",
    "MessageSchema",
    "Service",
    "ServiceRequest",
    "ServiceSchema",
]
