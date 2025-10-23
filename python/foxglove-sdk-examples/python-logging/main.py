import logging
import time

import foxglove

# The foxglove module provides a set_log_level function for convenience in scripts, which will call
# logging.basicConfig() for you. Many examples use that, but for more involved applications, you'll
# likely want to configure logging yourself.

# Debug level by default, and specify a format
logging.basicConfig(
    level=logging.DEBUG,
    format="%(asctime)s [%(levelname)s:%(name)s] %(message)s",
)

# You can still set the level of all Foxglove SDK logging. The SDK will no longer emit logs
# below the given level.
foxglove.set_log_level("INFO")

# To further filter SDK logs in a fine-grained way, you can get the logger by the module name; here,
# let's say we want to reduce the verbosity of the websocket server.
#
# (You could instead install a log filter on your handler or the root logger's handlers).
logging.getLogger("foxglove.websocket.server").setLevel(logging.WARN)

# A specific logger for this example module
logger = logging.getLogger("logging-example")


# With this configuration, we'll see debug app logging, but only info logs from Foxglove, and only
# warnings from the websocket server.
def main() -> None:
    server = foxglove.start_server()
    try:
        run_loop()
    except KeyboardInterrupt:
        server.stop()


def run_loop() -> None:
    count = 0
    while True:
        count += 1
        foxglove.log("/hello", {"message": "Hello, world!"})
        logger.debug("Logged message %d", count)
        time.sleep(1)


if __name__ == "__main__":
    main()
