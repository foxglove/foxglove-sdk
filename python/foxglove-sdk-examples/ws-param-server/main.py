"""
This implements a parameter server for live visualization.

View and edit parameters from a Parameters panel in Foxglove:
https://docs.foxglove.dev/docs/visualization/panels/parameters
"""

import logging
import time
from typing import List, Optional

import foxglove
from foxglove.websocket import Capability, Client, Parameter


class ParameterStore(foxglove.websocket.ServerListener):
    def __init__(self, parameters: list[Parameter]) -> None:
        # In this example our parameters are unique by name
        self.parameters = {param.name: param for param in parameters}
        # We can keep track of any parameters that are subscribed to
        self.subscribed_param_names: set[str] = set()

    # Foxglove server callback
    def on_get_parameters(
        self,
        client: Client,
        param_names: list[str],
        request_id: Optional[str] = None,
    ) -> list[Parameter]:
        logging.debug(f"on_get_parameters: {param_names}, {client.id}, {request_id}")
        if not param_names:
            return list(self.parameters.values())
        return [
            self.parameters[name] for name in param_names if name in self.parameters
        ]

    def on_set_parameters(
        self,
        client: Client,
        parameters: list[Parameter],
        request_id: Optional[str] = None,
    ) -> list[Parameter]:
        logging.debug(f"on_set_parameters: {parameters}, {client.id}, {request_id}")
        for changed_param in parameters:
            if changed_param.value is None:
                del self.parameters[changed_param.name]
            else:
                # Add or update
                self.parameters[changed_param.name] = changed_param
        return parameters

    def on_parameters_subscribe(self, param_names: List[str]) -> None:
        # The SDK takes care of notifying the client of the current parameters;
        # this is informational only.
        logging.debug(f"New subscriptions for: {param_names}")
        self.subscribed_param_names.update(param_names)

    def on_parameters_unsubscribe(self, param_names: List[str]) -> None:
        # The SDK takes care of notifying the client of the current parameters;
        # this is informational only.
        logging.debug(f"Remove subscriptions for: {param_names}")
        self.subscribed_param_names.difference_update(param_names)


def main() -> None:
    foxglove.set_log_level(logging.DEBUG)

    initial_values: list[Parameter] = [
        Parameter("p0"),
        Parameter(
            "p1",
            value={
                "a": 1,
                "b": True,
                "c": "hello",
                "arr": [1, True],
            },
        ),
        Parameter("p2", value=True),
        Parameter("p3", value=0.124),
        Parameter("p4", value=[1, 1, 2, 3, 5]),
        Parameter("p5", value=b"data"),
        Parameter("p6", value="hello"),
    ]

    store = ParameterStore(initial_values)

    websocket_server = foxglove.start_server(
        server_listener=store,
        capabilities=[
            # 'Parameters' is required for get/set callbacks
            Capability.Parameters,
        ],
    )

    try:
        while True:
            websocket_server.publish_parameter_values(list(store.parameters.values()))
            time.sleep(10)
    except KeyboardInterrupt:
        websocket_server.stop()


if __name__ == "__main__":
    main()
