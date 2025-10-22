from typing import Protocol

from ._foxglove_py.websocket import (
    ChannelView,
    Client,
    ClientChannel,
)


class CloudSinkListener(Protocol):
    """
    A mechanism to register callbacks for handling client message events.
    """

    def on_subscribe(self, client: Client, channel: ChannelView) -> None:
        """
        Called when a client subscribes to a channel.

        :param client: The client (id) that sent the message.
        :param channel: The channel (id, topic) that the message was sent on.
        """
        return None

    def on_unsubscribe(self, client: Client, channel: ChannelView) -> None:
        """
        Called when a client unsubscribes from a channel or disconnects.

        :param client: The client (id) that sent the message.
        :param channel: The channel (id, topic) that the message was sent on.
        """
        return None

    def on_client_advertise(self, client: Client, channel: ClientChannel) -> None:
        """
        Called when a client advertises a channel.

        :param client: The client (id) that sent the message.
        :param channel: The client channel that is being advertised.
        """
        return None

    def on_client_unadvertise(self, client: Client, client_channel_id: int) -> None:
        """
        Called when a client unadvertises a channel.

        :param client: The client (id) that is unadvertising the channel.
        :param client_channel_id: The client channel id that is being unadvertised.
        """
        return None

    def on_message_data(
        self, client: Client, client_channel_id: int, data: bytes
    ) -> None:
        """
        Called when a message is received from a client.

        :param client: The client (id) that sent the message.
        :param client_channel_id: The client channel id that the message was sent on.
        :param data: The message data.
        """
        return None
