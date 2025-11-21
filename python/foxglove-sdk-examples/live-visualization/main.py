import datetime
import json
import logging
import time
from math import cos, sin

import foxglove
import numpy as np
from foxglove import Channel, Schema
from foxglove.channels import RawImageChannel
from foxglove.schemas import (
    Color,
    CubePrimitive,
    Duration,
    FrameTransform,
    FrameTransforms,
    Pose,
    Quaternion,
    RawImage,
    SceneEntity,
    SceneUpdate,
    Timestamp,
    Vector3,
)
from foxglove.websocket import (
    Capability,
    ChannelView,
    Client,
    ClientChannel,
    ServerListener,
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


class ExampleListener(ServerListener):
    def __init__(self) -> None:
        # Map client id -> set of subscribed topics
        self.subscribers: dict[int, set[str]] = {}

    def has_subscribers(self) -> bool:
        return len(self.subscribers) > 0

    def on_subscribe(
        self,
        client: Client,
        channel: ChannelView,
    ) -> None:
        """
        Called by the server when a client subscribes to a channel.
        We'll use this and on_unsubscribe to simply track if we have any subscribers at all.
        """
        logging.info(f"Client {client} subscribed to channel {channel.topic}")
        self.subscribers.setdefault(client.id, set()).add(channel.topic)

    def on_unsubscribe(
        self,
        client: Client,
        channel: ChannelView,
    ) -> None:
        """
        Called by the server when a client unsubscribes from a channel.
        """
        logging.info(f"Client {client} unsubscribed from channel {channel.topic}")
        self.subscribers[client.id].remove(channel.topic)
        if not self.subscribers[client.id]:
            del self.subscribers[client.id]

    def on_client_advertise(
        self,
        client: Client,
        channel: ClientChannel,
    ) -> None:
        """
        Called when a client advertises a new channel.
        """
        logging.info(f"Client {client.id} advertised channel: {channel.id}")
        logging.info(f"  Topic: {channel.topic}")
        logging.info(f"  Encoding: {channel.encoding}")
        logging.info(f"  Schema name: {channel.schema_name}")
        logging.info(f"  Schema encoding: {channel.schema_encoding}")
        logging.info(f"  Schema: {channel.schema!r}")

    def on_message_data(
        self,
        client: Client,
        client_channel_id: int,
        data: bytes,
    ) -> None:
        """
        This handler demonstrates receiving messages from the client.
        You can send messages from Foxglove app in the publish panel:
        https://docs.foxglove.dev/docs/visualization/panels/publish
        """
        logging.info(f"Message from client {client.id} on channel {client_channel_id}")
        logging.info(f"Data: {data!r}")

    def on_client_unadvertise(
        self,
        client: Client,
        client_channel_id: int,
    ) -> None:
        """
        Called when a client unadvertises a new channel.
        """
        logging.info(f"Client {client.id} unadvertised channel: {client_channel_id}")


def main() -> None:
    foxglove.set_log_level(logging.DEBUG)

    listener = ExampleListener()

    server = foxglove.start_server(
        server_listener=listener,
        capabilities=[Capability.ClientPublish],
        supported_encodings=["json"],
    )

    # Log messages with a custom schema and any encoding.
    sin_chan = Channel(
        topic="/sine",
        message_encoding="json",
        schema=Schema(
            name="sine",
            encoding="jsonschema",
            data=json.dumps(plot_schema).encode("utf-8"),
        ),
    )

    # If you want to use JSON encoding, you can also specify the schema and log messages as dicts.
    # Dicts can also be logged without specifying a schema.
    json_chan = Channel(topic="/json", schema=plot_schema)

    img_chan = RawImageChannel(topic="/image")

    try:
        counter = 0
        while True:
            counter += 1
            now = time.time()
            y = sin(now)

            json_msg = {
                "timestamp": now,
                "y": y,
            }
            sin_chan.log(json.dumps(json_msg).encode("utf-8"))

            json_chan.log(json_msg)

            foxglove.log(
                "/tf",
                FrameTransforms(
                    transforms=[
                        FrameTransform(
                            parent_frame_id="world",
                            child_frame_id="box",
                            rotation=euler_to_quaternion(
                                roll=1, pitch=0, yaw=counter * 0.1
                            ),
                        ),
                    ]
                ),
            )

            foxglove.log(
                "/boxes",
                SceneUpdate(
                    entities=[
                        SceneEntity(
                            frame_id="box",
                            id="box_1",
                            timestamp=Timestamp.from_datetime(datetime.datetime.now()),
                            lifetime=Duration.from_secs(1.2345),
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
                ),
            )

            # Or use typed channels directly to get better type checking
            img_chan.log(
                RawImage(
                    data=np.zeros((100, 100, 3), dtype=np.uint8).tobytes(),
                    step=300,
                    width=100,
                    height=100,
                    encoding="rgb8",
                ),
            )

            time.sleep(0.05)

            while not listener.has_subscribers():
                time.sleep(1)
                continue

    except KeyboardInterrupt:
        server.stop()


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
