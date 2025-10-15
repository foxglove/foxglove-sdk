import inspect
import time

import foxglove
from foxglove.channels import LogChannel
from foxglove.schemas import Log, LogLevel

log_chan = LogChannel(topic="/log1")


def main() -> None:
    # Connect to Foxglove Agent for live visualization
    cloud = foxglove.start_cloud_sink()

    try:
        i = 0
        while True:
            frame = inspect.currentframe()
            frameinfo = inspect.getframeinfo(frame) if frame else None

            print(f"Logging message {i}")

            foxglove.log(
                "/log2",
                Log(
                    level=LogLevel.Info,
                    name="SDK example",
                    file=frameinfo.filename if frameinfo else None,
                    line=frameinfo.lineno if frameinfo else None,
                    message=f"message {i}",
                ),
            )

            # Or use a typed channel directly to get better type checking
            log_chan.log(
                Log(
                    level=LogLevel.Info,
                    name="SDK example",
                    file=frameinfo.filename if frameinfo else None,
                    line=frameinfo.lineno if frameinfo else None,
                    message=f"message {i}",
                ),
            )

            i += 1
            time.sleep(1)
    except KeyboardInterrupt:
        print("\nShutting down...")
    finally:
        cloud.stop()


if __name__ == "__main__":
    main()
