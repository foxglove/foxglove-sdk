import logging
import os
import time
from pathlib import Path

import foxglove
from foxglove import Channel, Schema
from foxglove.messages import (
    FrameTransform,
    FrameTransforms,
    Quaternion,
    Timestamp,
    Vector3,
)

ASSET_ROOT = Path(__file__).parent.resolve()
ROBOT_DESCRIPTION_TOPIC = "/robot_description"
ROBOT_DESCRIPTION = (ASSET_ROOT / "demo_robot" / "robot.urdf").read_text()


def asset_handler(uri: str) -> bytes | None:
    """Serve package assets from this example's directory."""
    if not uri.startswith("package://"):
        return None

    path = (ASSET_ROOT / uri.removeprefix("package://")).resolve()
    if not path.is_relative_to(ASSET_ROOT) or not path.is_file():
        logging.info(f"Asset not found: {uri}")
        return None

    logging.info(f"Serving asset: {uri}")
    return path.read_bytes()


def main() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )
    foxglove.set_log_level("INFO")

    robot_description_channel = Channel(
        ROBOT_DESCRIPTION_TOPIC,
        message_encoding="json",
        schema=Schema(
            name="std_msgs/msg/String",
            encoding="jsonschema",
            data=(
                b'{"type":"object","properties":{"data":{"type":"string"}},'
                b'"required":["data"]}'
            ),
        ),
    )

    server = foxglove.start_server(
        name="asset-server",
        asset_handler=asset_handler,
    )
    gateway = None

    try:
        device_token = os.getenv("FOXGLOVE_DEVICE_TOKEN")
        if device_token:
            gateway = foxglove.start_gateway(
                name="asset-server",
                device_token=device_token,
                asset_handler=asset_handler,
            )

        while True:
            robot_description_channel.log({"data": ROBOT_DESCRIPTION})
            foxglove.log(
                "/tf",
                FrameTransforms(
                    transforms=[
                        FrameTransform(
                            timestamp=Timestamp.from_epoch_secs(time.time()),
                            parent_frame_id="world",
                            child_frame_id="base_link",
                            translation=Vector3(x=0.0, y=0.0, z=0.0),
                            rotation=Quaternion(x=0.0, y=0.0, z=0.0, w=1.0),
                        )
                    ]
                ),
            )
            time.sleep(1)
    except KeyboardInterrupt:
        pass
    finally:
        if gateway is not None:
            gateway.stop()
        server.stop()


if __name__ == "__main__":
    main()
