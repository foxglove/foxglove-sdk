import math
import time

import foxglove
from foxglove import Channel
from foxglove.channels import SceneUpdateChannel
from foxglove.schemas import (
    Color,
    CubePrimitive,
    SceneEntity,
    SceneUpdate,
    Vector3,
)

foxglove.set_log_level("DEBUG")

# Our example logs data on a couple of different topics, so we'll create a
# channel for each. We can use a channel like SceneUpdateChannel to log
# Foxglove schemas, or a generic Channel to log custom data.
scene_channel = SceneUpdateChannel("/scene")
size_channel = Channel("/size", message_encoding="json")

# We'll log to both an MCAP file, and to a running Foxglove app via a server.
file_name = "quickstart-python.mcap"
writer = foxglove.open_mcap(file_name)
server = foxglove.start_server()

while True:
    size = abs(math.sin(time.time())) + 1

    # Log messages on both channels until interrupted. By default, each message
    # is stamped with the current time.
    size_channel.log({"size": size})
    scene_channel.log(
        SceneUpdate(
            entities=[
                SceneEntity(
                    cubes=[
                        CubePrimitive(
                            size=Vector3(x=size, y=size, z=size),
                            color=Color(r=1.0, g=0, b=0, a=1.0),
                        )
                    ],
                ),
            ]
        )
    )

    time.sleep(0.033)
