import json
import logging
import math
import struct
import time
from math import cos, sin

import foxglove
import numpy as np
from foxglove import Capability, SchemaDefinition
from foxglove.channels import (
    FrameTransformsChannel,
    PointCloudChannel,
    SceneUpdateChannel,
)
from foxglove.schemas import (
    Color,
    CubePrimitive,
    Duration,
    FrameTransform,
    FrameTransforms,
    PackedElementField,
    PackedElementFieldNumericType,
    PointCloud,
    Pose,
    Quaternion,
    RawImage,
    SceneEntity,
    SceneUpdate,
    Vector3,
)

any_schema = {
    "type": "object",
    "additionalProperties": True,
}

plot_schema = {
    "type": "object",
    "properties": {
        "timestamp": {"type": "number"},
        "y": {"type": "number"},
    },
}


class ExampleListener(foxglove.ServerListener):
    def __init__(self) -> None:
        self.subscribers: set[int] = set()

    def has_subscribers(self) -> bool:
        return len(self.subscribers) > 0

    def on_subscribe(
        self,
        client: foxglove.Client,
        channel: foxglove.ChannelView,
    ) -> None:
        """
        Called by the server when a client subscribes to a channel.
        We'll use this and on_unsubscribe to simply track if we have any subscribers at all.
        """
        logging.info(f"Client {client} subscribed to channel {channel.topic}")
        self.subscribers.add(client.id)

    def on_unsubscribe(
        self,
        client: foxglove.Client,
        channel: foxglove.ChannelView,
    ) -> None:
        """
        Called by the server when a client unsubscribes from a channel.
        """
        logging.info(f"Client {client} unsubscribed from channel {channel.topic}")
        self.subscribers.remove(client.id)

    def on_message_data(
        self,
        client: foxglove.Client,
        channel: foxglove.ChannelView,
        data: bytes,
    ) -> None:
        """
        This handler demonstrates receiving messages from the client.
        You can send messages from Foxglove app in the publish panel:
        https://docs.foxglove.dev/docs/visualization/panels/publish
        """
        logging.info(f"Message from client {client.id} on channel {channel.topic}")
        logging.info(f"Data: {data!r}")


def main() -> None:
    foxglove.verbose_on()

    listener = ExampleListener()

    server = foxglove.start_server(
        server_listener=listener,
        capabilities=[Capability.ClientPublish],
        supported_encodings=["json"],
    )

    # Log messages having well-known Foxglove schemas using the appropriate channel type.
    box_chan = SceneUpdateChannel("/boxes")
    tf_chan = FrameTransformsChannel("/tf")
    point_chan = PointCloudChannel("/pointcloud")

    # Log dicts using JSON encoding
    json_chan = foxglove.Channel(topic="/json", schema=plot_schema)

    # Log messages with a custom schema and any encoding
    sin_chan = foxglove.Channel(
        topic="/sine",
        schema=SchemaDefinition(
            name="sine",
            schema_encoding="jsonschema",
            message_encoding="json",
            schema_data=json.dumps(plot_schema).encode("utf-8"),
        ),
    )

    try:
        counter = 0
        while True:
            if not listener.has_subscribers():
                continue

            counter += 1
            now = time.time()
            y = np.sin(now)

            json_msg = {
                "timestamp": now,
                "y": y,
            }
            sin_chan.log(json.dumps(json_msg).encode("utf-8"))

            json_chan.log(json_msg)

            tf_chan.log(
                FrameTransforms(
                    transforms=[
                        FrameTransform(
                            parent_frame_id="world",
                            child_frame_id="box",
                            rotation=euler_to_quaternion(
                                roll=1, pitch=0, yaw=counter * 0.1
                            ),
                        ),
                        FrameTransform(
                            parent_frame_id="world",
                            child_frame_id="points",
                            translation=Vector3(x=-10, y=-10, z=0),
                        ),
                    ]
                )
            )

            box_chan.log(
                SceneUpdate(
                    entities=[
                        SceneEntity(
                            frame_id="box",
                            id="box_1",
                            lifetime=Duration(seconds=10),
                            cubes=[
                                CubePrimitive(
                                    pose=Pose(
                                        position=Vector3(x=0, y=y, z=3),
                                        orientation=euler_to_quaternion(
                                            roll=0, pitch=0, yaw=counter * -0.1
                                        ),
                                    ),
                                    size=Vector3(x=1, y=1, z=1),
                                    color=Color(r=1.0, g=0, b=0, a=1),
                                )
                            ],
                        ),
                    ]
                )
            )

            point_chan.log(make_point_cloud())

            # Or use high-level log API without needing to manage explicit Channels.
            foxglove.log(
                "/high-level",
                RawImage(
                    data=np.zeros((100, 100, 3), dtype=np.uint8).tobytes(),
                    step=300,
                    width=100,
                    height=100,
                    encoding="rgb8",
                ),
            )

            time.sleep(0.05)

    except KeyboardInterrupt:
        server.stop()


def make_point_cloud() -> PointCloud:
    """
    https://foxglove.dev/blog/visualizing-point-clouds-with-custom-colors
    """
    point_struct = struct.Struct("<fffBBBB")
    f32 = PackedElementFieldNumericType.Float32
    u32 = PackedElementFieldNumericType.Uint32

    t = time.time()
    points = [(x + math.cos(t + y / 5), y, 0) for x in range(20) for y in range(20)]
    buffer = bytearray(point_struct.size * len(points))
    for i, point in enumerate(points):
        x, y, z = point
        r = int(255 * (0.5 + 0.5 * x / 20))
        g = int(255 * y / 20)
        b = int(255 * (0.5 + 0.5 * math.sin(t)))
        a = int(255 * (0.5 + 0.5 * ((x / 20) * (y / 20))))
        point_struct.pack_into(buffer, i * point_struct.size, x, y, z, b, g, r, a)

    return PointCloud(
        frame_id="points",
        pose=Pose(
            position=Vector3(x=0, y=0, z=0),
            orientation=Quaternion(x=0, y=0, z=0, w=1),
        ),
        point_stride=16,  # 4 fields * 4 bytes
        fields=[
            PackedElementField(name="x", offset=0, type=f32),
            PackedElementField(name="y", offset=4, type=f32),
            PackedElementField(name="z", offset=8, type=f32),
            PackedElementField(name="rgba", offset=12, type=u32),
        ],
        data=bytes(buffer),
    )


def euler_to_quaternion(roll: float, pitch: float, yaw: float) -> Quaternion:
    """Convert Euler angles to a rotation quaternion

    See e.g. https://danceswithcode.net/engineeringnotes/quaternions/quaternions.html

    :param roll: rotation around X axis (radians)
    :param pitch: rotation around Y axis (radians)
    :param yaw: rotation around Z axis (radians)
    :returns: a protobuf Quaternion
    """
    roll, pitch, yaw = roll * 0.5, pitch * 0.5, yaw * 0.5

    sin_r, cos_r = sin(roll), cos(roll)
    sin_p, cos_p = sin(pitch), cos(pitch)
    sin_y, cos_y = sin(yaw), cos(yaw)

    w = cos_r * cos_p * cos_y + sin_r * sin_p * sin_y
    x = sin_r * cos_p * cos_y - cos_r * sin_p * sin_y
    y = cos_r * sin_p * cos_y + sin_r * cos_p * sin_y
    z = cos_r * cos_p * sin_y - sin_r * sin_p * cos_y

    return Quaternion(x=x, y=y, z=z, w=w)


if __name__ == "__main__":
    main()
