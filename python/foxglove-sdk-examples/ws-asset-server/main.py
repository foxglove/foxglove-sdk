import logging
import time
from typing import Optional

import foxglove


def asset_handler(uri: str) -> Optional[bytes]:
    """
    This will respond to "package://" asset requests from Foxglove by reading files from disk.
    This example doesn't do any path validation or upward traversal prevention.
    """
    asset = None
    if uri.startswith("package://"):
        filepath = uri.replace("package://", "", 1)
        try:
            with open(filepath, "rb") as file:
                asset = file.read()
        except FileNotFoundError:
            pass

    status = "OK" if asset else "Not Found"
    logging.debug(f"asset_handler {status}: {uri}")
    return asset


def main() -> None:
    foxglove.set_log_level(logging.DEBUG)

    server = foxglove.start_server(
        asset_handler=asset_handler,
    )

    try:
        while True:
            # Send transforms for the model as needed, on a `FrameTransformsChannel`
            time.sleep(1)

    except KeyboardInterrupt:
        server.stop()


if __name__ == "__main__":
    main()
