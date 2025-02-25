"""
This example demonstrates how to use the Foxglove WebSocket API to implement services which can be
called from the Service Call panel in the Foxglove app.

https://docs.foxglove.dev/docs/visualization/panels/service-call
"""

import argparse
import logging

from foxglove import (
    Capability,
    Client,
    Request,
    Service,
    ServiceSchema,
    start_server,
)


# A handler can also be a bare function.
def logging_handler(
    client: Client,
    request: Request,
) -> bytes:
    """
    A handler for the service, adhering to the `ServiceHandler` type.

    The handler should return a bytes object which will be sent back to the client.
    """
    log_request(client, request)
    return b"{}"


# A handler can also be defined as any callable.
class EchoService:
    def __call__(
        self,
        client: Client,
        request: Request,
    ) -> bytes:
        log_request(client, request)
        return request.payload


def log_request(client: Client, request: Request):
    logging.debug(
        f"[{request.service_name}] Request {request.call_id} from {client} on {request.encoding}: "
        f"{request.payload!r}"
    )


def main():
    """
    This example demonstrates how to use the Foxglove WebSocket API to implement services which can
    be called from the Foxglove app.
    """
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

    server = start_server(
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
