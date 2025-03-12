import logging
import time

import foxglove
from foxglove.websocket import Capability, ConnectionGraph, ServerListener


class SubscriptionWatcher(ServerListener):
    """
    A server listener that keeps track of whether any clients are subscribed to the connection graph
    """

    def __init__(self) -> None:
        self.has_subscribers = False

    def on_connection_graph_subscribe(self) -> None:
        logging.debug("on_connection_graph_subscribe")
        self.has_subscribers = True

    def on_connection_graph_unsubscribe(self) -> None:
        logging.debug("on_connection_graph_unsubscribe")
        self.has_subscribers = False


def main() -> None:
    foxglove.set_log_level("DEBUG")

    graph = ConnectionGraph()
    graph.set_published_topic("/example-topic", ["1", "2"])
    graph.set_subscribed_topic("/subscribed-topic", ["3", "4"])
    graph.set_advertised_service("example-service", ["5", "6"])

    logging.debug(graph)

    server = foxglove.start_server(
        server_listener=SubscriptionWatcher(),
        capabilities=[Capability.ConnectionGraph],
    )

    try:
        while True:
            server.publish_connection_graph(graph)
            time.sleep(1)
    except KeyboardInterrupt:
        server.stop()


if __name__ == "__main__":
    main()
