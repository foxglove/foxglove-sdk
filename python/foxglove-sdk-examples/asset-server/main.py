import logging
import os
import time
from pathlib import Path

import foxglove
from foxglove import Parameter
from foxglove.messages import (
    FrameTransform,
    FrameTransforms,
    Quaternion,
    Timestamp,
    Vector3,
)
from foxglove.remote_access import Capability as GatewayCapability
from foxglove.remote_access import Client as GatewayClient
from foxglove.remote_access import RemoteAccessListener
from foxglove.websocket import Capability as WebSocketCapability
from foxglove.websocket import Client as WebSocketClient
from foxglove.websocket import ServerListener

ASSET_ROOT = Path(__file__).parent.resolve()
ROBOT_DESCRIPTION_PARAM = "/robot_description"
ROBOT_DESCRIPTION = Parameter(
    ROBOT_DESCRIPTION_PARAM,
    value=(ASSET_ROOT / "demo_robot" / "robot.urdf").read_text(),
)


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


def get_parameters(param_names: list[str]) -> list[Parameter]:
    """Serve the URDF XML as the /robot_description parameter.

    An empty request means "all parameters".
    """
    if param_names and ROBOT_DESCRIPTION_PARAM not in param_names:
        return []
    return [ROBOT_DESCRIPTION]


class WebSocketParameterServer(ServerListener):
    def on_get_parameters(
        self,
        client: WebSocketClient,
        param_names: list[str],
        request_id: str | None = None,
    ) -> list[Parameter]:
        return get_parameters(param_names)


class GatewayParameterServer(RemoteAccessListener):
    def on_get_parameters(
        self,
        client: GatewayClient,
        param_names: list[str],
        request_id: str | None = None,
    ) -> list[Parameter]:
        return get_parameters(param_names)


def main() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )
    foxglove.set_log_level("INFO")

    server = foxglove.start_server(
        name="asset-server",
        capabilities=[WebSocketCapability.Parameters],
        server_listener=WebSocketParameterServer(),
        asset_handler=asset_handler,
    )

    device_token = os.getenv("FOXGLOVE_DEVICE_TOKEN")
    if device_token:
        gateway = foxglove.start_gateway(
            name="asset-server",
            device_token=device_token,
            capabilities=[GatewayCapability.Parameters],
            listener=GatewayParameterServer(),
            asset_handler=asset_handler,
        )
    else:
        gateway = None

    try:
        while True:
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
