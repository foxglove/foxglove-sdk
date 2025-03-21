import json
import math
import time

import foxglove
from foxglove.channel import Channel
from foxglove.channels import SceneUpdateChannel
from foxglove.schemas import (
    Color,
    CubePrimitive,
    SceneEntity,
    SceneUpdate,
    Vector3,
)

foxglove.set_log_level("DEBUG")

scene_channel = SceneUpdateChannel("/scene")
json_channel = Channel("/info", message_encoding="json")

file_name = "quickstart-python.mcap"
writer = foxglove.open_mcap(file_name)
server = foxglove.start_server()

while True:
    size = abs(math.sin(time.time())) + 1

    json_channel.log(json.dumps({"size": size}).encode("utf-8"))
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
