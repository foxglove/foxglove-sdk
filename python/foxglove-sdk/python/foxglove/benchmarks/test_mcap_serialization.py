import json
import math
import os
import struct
import time
from pathlib import Path
from typing import Generator, List

import pytest
from foxglove import Channel, open_mcap
from foxglove.channels import PointCloudChannel, SceneUpdateChannel
from foxglove.schemas import (
    Color,
    CubePrimitive,
    Duration,
    PackedElementField,
    PackedElementFieldNumericType,
    PointCloud,
    Pose,
    Quaternion,
    SceneEntity,
    SceneUpdate,
    Vector3,
)
from pytest_benchmark.fixture import BenchmarkFixture  # type: ignore


@pytest.fixture
def tmp_mcap(tmpdir: os.PathLike[str]) -> Generator[Path, None, None]:
    dir = Path(tmpdir)
    mcap = dir / "test.mcap"
    yield mcap
    mcap.unlink()
    dir.rmdir()


def build_entities(entity_count: int) -> List[SceneEntity]:
    assert entity_count > 0
    return [
        SceneEntity(
            id=f"box_{i}",
            frame_id="box",
            lifetime=Duration(10, nsec=int(100 * 1e6)),
            cubes=[
                CubePrimitive(
                    pose=Pose(
                        position=Vector3(x=0.0, y=0.0, z=3.0),
                        orientation=Quaternion(x=0.0, y=0.0, z=0.0, w=1.0),
                    ),
                    size=Vector3(x=1.0, y=1.0, z=1.0),
                    color=Color(r=1.0, g=0.0, b=0.0, a=1.0),
                )
            ],
        )
        for i in range(entity_count)
    ]


def write_scene_entity_mcap(
    tmp_mcap: Path, channel: SceneUpdateChannel, entities: List[SceneEntity]
) -> None:
    with open_mcap(tmp_mcap, allow_overwrite=True):
        for _ in range(100):
            channel.log(SceneUpdate(entities=entities))


def make_point_cloud(point_count: int) -> PointCloud:
    """
    https://foxglove.dev/blog/visualizing-point-clouds-with-custom-colors
    """
    point_struct = struct.Struct("<fffBBBB")
    f32 = PackedElementFieldNumericType.Float32
    u32 = PackedElementFieldNumericType.Uint32

    t = time.time()
    count = math.ceil(math.sqrt(point_count))
    points = [
        (x + math.cos(t + y / 5), y, 0) for x in range(count) for y in range(count)
    ]

    buffer = bytearray(point_struct.size * len(points))
    for i, point in enumerate(points):
        x, y, z = point
        r = g = b = a = 128
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


def write_point_cloud_mcap(
    tmp_mcap: Path, channel: PointCloudChannel, point_cloud: PointCloud
) -> None:
    with open_mcap(tmp_mcap, allow_overwrite=True):
        for _ in range(10):
            channel.log(point_cloud)


def write_untyped_channel_mcap(
    tmp_mcap: Path, channel: Channel, messages: List[bytes]
) -> None:
    with open_mcap(tmp_mcap, allow_overwrite=True):
        for message in messages:
            channel.log(message)


@pytest.mark.benchmark
@pytest.mark.parametrize("entity_count", [1, 2, 4, 8])
def test_write_scene_update_mcap(
    benchmark: BenchmarkFixture,
    entity_count: int,
    tmp_mcap: Path,
) -> None:
    channel = SceneUpdateChannel(f"/scene_{entity_count}")
    entities = build_entities(entity_count)
    benchmark(write_scene_entity_mcap, tmp_mcap, channel, entities)


@pytest.mark.benchmark
@pytest.mark.parametrize("point_count", [100, 1000, 10000])
def test_write_point_cloud_mcap(
    benchmark: BenchmarkFixture,
    point_count: int,
    tmp_mcap: Path,
) -> None:
    print("test_write_point_cloud_mcap")
    channel = PointCloudChannel(f"/point_cloud_{point_count}")
    point_cloud = make_point_cloud(point_count)
    benchmark(write_point_cloud_mcap, tmp_mcap, channel, point_cloud)


@pytest.mark.benchmark
@pytest.mark.parametrize("message_count", [10, 100, 1000])
def test_write_untyped_channel_mcap(
    benchmark: BenchmarkFixture,
    message_count: int,
    tmp_mcap: Path,
) -> None:
    channel = Channel(
        f"/untyped_{message_count}",
        schema={"type": "object", "additionalProperties": True},
    )
    messages = [
        json.dumps({"message": f"hello_{i}"}).encode("utf-8")
        for i in range(message_count)
    ]
    benchmark(write_untyped_channel_mcap, tmp_mcap, channel, messages)
