# This is a test file, do not commit to main branch
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
from foxglove.layouts import *
import os

# Extract topic names as variables to be used in the layout and when creating the channels
sceneTopicName = "/scene"
sizeTopicName = "/size"

three_dee_panel = ThreeDeePanel(
    config=ThreeDeeConfig(
        topics={
            sceneTopicName: BaseRendererSceneUpdateTopicSettings(visible=True)
        },
        layers={
            "grid": BaseRendererGridLayerSettings()
        }
    ),
)

split = SplitContainer(
    direction="row",
    items=[
        SplitItem(content=three_dee_panel),
        SplitItem(content=RawMessagesPanel(
            config=RawMessagesConfig(
                topic_path=sizeTopicName
            )
        )),
    ]
)

full_layout = Layout(
  content=split,
)

scene_layout = Layout(content=three_dee_panel)

foxglove.set_log_level("DEBUG")

# Our example logs data on a couple of different topics, so we'll create a
# channel for each. We can use a channel like SceneUpdateChannel to log
# Foxglove schemas, or a generic Channel to log custom data.
scene_channel = SceneUpdateChannel(sceneTopicName)
size_channel = Channel(sizeTopicName, message_encoding="json")

# We'll log to an MCAP file
file_name = "mcap_with_multiple_layout.mcap"

# remove the file if it exists
if os.path.exists(file_name):
    os.remove(file_name)

# Close the mcap writer with close() or the with statement
with foxglove.open_mcap(file_name) as writer:
    # Write the layout to the MCAP file
    writer.write_layout("Full view", full_layout.to_json())
    writer.write_layout("Scene view", scene_layout.to_json())

    # Log 10 seconds worth of messages
    for _ in range(10*30):
        size = abs(math.sin(time.time())) + 1

        # Log messages on both channels. By default, each message
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
