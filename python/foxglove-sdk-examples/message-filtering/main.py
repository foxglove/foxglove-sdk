import logging
import struct
import time
from math import cos, sin

import foxglove
from foxglove import FilterableChannel
from foxglove.schemas import (
    FrameTransform,
    FrameTransforms,
    PackedElementField,
    PackedElementFieldNumericType,
    PointCloud,
    Pose,
    Quaternion,
    Vector3,
)


def keep_large_topics(channel: FilterableChannel) -> bool:
    return channel.topic.startswith("/point_cloud")


def drop_large_topics(channel: FilterableChannel) -> bool:
    return not keep_large_topics(channel)


# We'll send all messages to the Foxglove app. We don't need a filter for this, since its the same
# as having no filter applied, but this demonstrates how to apply a filter to the server.
def live_viz_filter(_: FilterableChannel) -> bool:
    return True


def main() -> None:
    foxglove.set_log_level(logging.DEBUG)

    small_mcap = foxglove.open_mcap(
        "example-topic-splitting-small.mcap", channel_filter=drop_large_topics
    )
    large_mcap = foxglove.open_mcap(
        "example-topic-splitting-large.mcap", channel_filter=keep_large_topics
    )
    server = foxglove.start_server(
        channel_filter=live_viz_filter,
    )

    cloud_tf = FrameTransforms(
        transforms=[
            FrameTransform(
                parent_frame_id="world",
                child_frame_id="points",
                translation=Vector3(x=-10, y=-10, z=0),
            ),
        ]
    )

    try:
        while True:
            foxglove.log("/info", {"state": get_state(), "y": cos(time.time())})

            foxglove.log("/point_cloud_tf", cloud_tf)

            foxglove.log(
                "/point_cloud",
                make_point_cloud(),
            )

            time.sleep(0.05)

    except KeyboardInterrupt:
        server.stop()
        small_mcap.close()
        large_mcap.close()


def get_state() -> str:
    value = cos(time.time())
    return "pos" if value > 0 else "neg"


def make_point_cloud() -> PointCloud:
    """
    https://foxglove.dev/blog/visualizing-point-clouds-with-custom-colors
    """
    point_struct = struct.Struct("<fffBBBB")
    f32 = PackedElementFieldNumericType.Float32
    u32 = PackedElementFieldNumericType.Uint32

    t = time.time()
    points = [(x + cos(t + y / 5), y, 0) for x in range(20) for y in range(20)]
    buffer = bytearray(point_struct.size * len(points))
    for i, point in enumerate(points):
        x, y, z = point
        r = int(255 * (0.5 + 0.5 * x / 20))
        g = int(255 * y / 20)
        b = int(255 * (0.5 + 0.5 * sin(t)))
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


if __name__ == "__main__":
    main()
