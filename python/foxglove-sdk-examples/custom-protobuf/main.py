import argparse
import base64
import json

import foxglove
import fruit_pb2
from foxglove import Channel, Schema
from foxglove.channels import CompressedImageChannel
from foxglove.schemas import CompressedImage
from google.protobuf import descriptor_pb2

parser = argparse.ArgumentParser()
parser.add_argument("--path", type=str, default="output.mcap")
args = parser.parse_args()

# this channel logs images using Foxglove's image schema
img_channel = CompressedImageChannel(topic="/image")

# this channel logs schemaless JSON
schemaless_channel = Channel("/schemaless", message_encoding="json")

# this channel logs JSON with a jsonschema
point_schema = {
    "type": "object",
    "properties": {
        "x": {"type": "number"},
        "y": {"type": "number"},
    },
}
points_channel = Channel(
    "/points",
    message_encoding="json",
    schema=Schema(
        name="point",
        encoding="jsonschema",
        data=json.dumps(point_schema).encode("utf-8"),
    ),
)

# this channel uses a custom protobuf schema
proto_fds = descriptor_pb2.FileDescriptorSet()
fruit_pb2.DESCRIPTOR.CopyToProto(proto_fds.file.add())
apple_descriptor = fruit_pb2.Apple.DESCRIPTOR
proto_chan = Channel(
    topic="/proto",
    message_encoding="protobuf",
    schema=Schema(
        name=f"{apple_descriptor.file.package}.{apple_descriptor.name}",
        encoding="protobuf",
        data=proto_fds.SerializeToString(),
    ),
)

IMG_DATA = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAYAAACqaXHeAAAB3klEQVR4nO1b25aDMAjEPfv/v2wfttlDaW4QkCjMW7Ulw2SIqcoB1+Fkfv8wYXHhIB8Jn8z0j29mJlwtgp4A/IRHQIKoctYMZpI4hbYQGkEuSZxCS4iVH7skTrEqhFSA0ztxircQ7Hx+BGNtlzzAvxPZzDiKbWH5EbglMSvAlrPew2xJzJTA7ZIHmC8JyRrwKIwEuOXsF8y4oCfArZMvGInQEuARyRf0RMg1oHLsUbNf0HJBeAfQjYLK7OObGbvFoxuk37Vw1eDVYxLi2vFqwCXwyNqnoGuB2hpQmy3Oeet4LRQBQsx+AXZB+KuAmgAjB3Edph2vhfAOOMCg/nfeB9C4qvuAAm1BLRfo8CWQAngT8EYK4E3AGymANwFvpADeBLyRAgDAoXVz4U4o9wbTAd4EvBFeAFz9ovsCVv/VLcfFzwbCOwALEOJqQJ8MpQPIZ7YLcP1d5SBp/ddenEoHVI5t7QLN2QcwcoCVCBZxWwIsueAvgIhPEzSexuwD9B2wjQhWyXdPfIy33u6i9oKEZvIAhk+GKPnZxavnGouttunL0prrwMp+v/s9DgcJEURGhIXyU31dHmPpafKMGNL4kq6R8C0z2TSlwCFs2xxF2MZJirCtsxRhm6db2LJ9/gUqza1n1/8fpgAAAABJRU5ErkJggg=="  # noqa: E501
)


def main() -> None:
    # Create a new mcap file at the given path for recording
    with foxglove.open_mcap(args.path, allow_overwrite=True):
        for i in range(100):
            # a very simple png image
            img_channel.log(
                CompressedImage(data=IMG_DATA, format="png"), log_time=i * 100_000_000
            )

            # for JSON channels we can just pass a dict
            schemaless_channel.log({"foo": f"Hello {i}!"}, log_time=i * 100_000_000)
            points_channel.log({"x": i, "y": i * 2}, log_time=i * 100_000_000)

            # Create and log a protobuf message
            apple = fruit_pb2.Apple()
            apple.color = "red"
            apple.diameter = 10 * i
            proto_chan.log(apple.SerializeToString(), log_time=i * 100_000_000)


if __name__ == "__main__":
    main()
