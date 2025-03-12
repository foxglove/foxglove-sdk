"""
This example demonstrates how to use the Foxglove WebSocket API to implement services which can be
called from the Service Call panel in the Foxglove app.

https://docs.foxglove.dev/docs/visualization/panels/service-call
"""

import argparse
import logging

import foxglove
from foxglove.websocket import (
    Capability,
    Service,
    ServiceRequest,
    ServiceSchema,
)


# A handler can also be a bare function.
def logging_handler(
    request: ServiceRequest,
) -> bytes:
    """
    A handler for the service, adhering to the `ServiceHandler` type.

    The handler should return a bytes object which will be sent back to the client.
    """
    log_request(request)
    return b"{}"


# A handler can also be defined as any callable.
class EchoService:
    def __call__(
        self,
        request: ServiceRequest,
    ) -> bytes:
        log_request(request)
        return request.payload


def log_request(r: ServiceRequest):
    logging.debug(
        f"[{r.service_name}] "
        f"client {r.client_id} call {r.call_id}: "
        f"({r.encoding}): {r.payload!r}"
    )


def main():
    """
    This example demonstrates how to use the Foxglove WebSocket API to implement services which can
    be called from the Foxglove app.
    """
    foxglove.set_log_level("DEBUG")

    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--host", type=str, default="127.0.0.1")
    args = parser.parse_args()

    logging_service = Service(
        name="logging",
        schema=ServiceSchema(
            name="logging-schema",
        ),
        handler=logging_handler,
    )

    echo_service = Service(
        name="echo",
        schema=ServiceSchema(
            name="echo-schema",
        ),
        handler=EchoService(),
    )

    server = foxglove.start_server(
        name="ws-services-example",
        port=args.port,
        host=args.host,
        capabilities=[Capability.Services],
        # If publishing from Foxglove, add at least one supported encoding (json, ros1, or cdr).
        # These examples use json.
        supported_encodings=["json"],
        # The services to publish
        services=[echo_service, logging_service],
    )

    try:
        while True:
            pass
    except KeyboardInterrupt:
        server.stop()


if __name__ == "__main__":
    main()
