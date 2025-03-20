import argparse
import inspect

import foxglove
from foxglove.schemas import Log, LogLevel

parser = argparse.ArgumentParser()
parser.add_argument("--path", type=str, default="output.mcap")
args = parser.parse_args()


def main() -> None:
    # Create a new mcap file at the given path for recording
    with foxglove.open_mcap(args.path):
        for i in range(10):
            frame = inspect.currentframe()
            frameinfo = inspect.getframeinfo(frame) if frame else None

            foxglove.log(
                "/hello",
                Log(
                    level=LogLevel.Info,
                    name="SDK example",
                    file=frameinfo.filename if frameinfo else None,
                    line=frameinfo.lineno if frameinfo else None,
                    message=f"message {i}",
                ),
            )


if __name__ == "__main__":
    main()
