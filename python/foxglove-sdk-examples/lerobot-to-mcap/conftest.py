import json
import os
from collections.abc import Callable, Sequence
from pathlib import Path
from typing import Any

import av
import numpy as np
import pyarrow as pa
import pyarrow.parquet as pq
import pytest
from av.video.stream import VideoStream

FPS = 10
WIDTH = 64
HEIGHT = 48
CAMERA = "observation.images.front"
TASKS = ["pick up the cube", "put the cube down"]
CODEC_OPTIONS = {"libsvtav1": {"preset": "12"}, "libx264": {"bf": "0"}}
SVT_LOG_ERRORS_ONLY = "1"

os.environ.setdefault("SVT_LOG", SVT_LOG_ERRORS_ONLY)


def _features(camera_dtype: str) -> dict[str, Any]:
    return {
        CAMERA: {
            "dtype": camera_dtype,
            "shape": [HEIGHT, WIDTH, 3],
            "names": ["height", "width", "channels"],
        },
        "observation.state": {
            "dtype": "float32",
            "shape": [2],
            "names": ["shoulder.pos", "gripper.pos"],
        },
        "action": {
            "dtype": "float32",
            "shape": [2],
            "names": {"motors": ["shoulder", "gripper"]},
        },
        "next.reward": {"dtype": "float32", "shape": [1], "names": None},
        "next.done": {"dtype": "bool", "shape": [1], "names": None},
        "timestamp": {"dtype": "float32", "shape": [1], "names": None},
        "frame_index": {"dtype": "int64", "shape": [1], "names": None},
        "episode_index": {"dtype": "int64", "shape": [1], "names": None},
        "index": {"dtype": "int64", "shape": [1], "names": None},
        "task_index": {"dtype": "int64", "shape": [1], "names": None},
    }


def _pixels(frame: int) -> np.ndarray:
    return np.full((HEIGHT, WIDTH, 3), frame * 20 % 256, dtype=np.uint8)


def _encode_video(
    path: Path, frames: int, codec: str, gop: int, options: dict[str, str] | None
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with av.open(str(path), "w") as container:
        stream = container.add_stream(codec, rate=FPS)
        assert isinstance(stream, VideoStream)
        stream.width = WIDTH
        stream.height = HEIGHT
        stream.pix_fmt = "yuv420p"
        stream.gop_size = gop
        stream.options = {
            **(CODEC_OPTIONS.get(codec, {}) if options is None else options)
        }
        for index in range(frames):
            frame = av.VideoFrame.from_ndarray(_pixels(index), format="rgb24")
            container.mux(stream.encode(frame))
        container.mux(stream.encode(None))


def _encode_png(frame: int) -> bytes:
    codec = av.CodecContext.create("png", "w")
    codec.width = WIDTH
    codec.height = HEIGHT
    codec.pix_fmt = "rgb24"
    image = av.VideoFrame.from_ndarray(_pixels(frame), format="rgb24")
    return b"".join(bytes(packet) for packet in codec.encode(image))


def _frames(
    episode: int, first_index: int, task_indexes: Sequence[int], camera_dtype: str
) -> pa.Table:
    length = len(task_indexes)
    columns: dict[str, Any] = {
        "observation.state": pa.array(
            [[i, i + 0.5] for i in range(length)], pa.list_(pa.float32())
        ),
        "action": pa.array(
            [[i + 1, i + 1.5] for i in range(length)], pa.list_(pa.float32())
        ),
        "next.reward": pa.array([i / 10 for i in range(length)], pa.float32()),
        "next.done": [i == length - 1 for i in range(length)],
        "timestamp": pa.array([i / FPS for i in range(length)], pa.float32()),
        "frame_index": list(range(length)),
        "episode_index": [episode] * length,
        "index": list(range(first_index, first_index + length)),
        "task_index": list(task_indexes),
    }
    if camera_dtype == "image":
        columns[CAMERA] = [
            {"bytes": _encode_png(i), "path": None} for i in range(length)
        ]
    return pa.table(columns)


def _write_parquet(path: Path, table: pa.Table) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    pq.write_table(table, path)


def write_dataset(
    root: Path,
    version: str,
    *,
    frame_task_indexes: Sequence[Sequence[int]] = ([0] * 6, [0] * 4),
    camera_dtype: str = "video",
    codec: str | None = None,
    gop: int = 2,
    video_options: dict[str, str] | None = None,
) -> Path:
    lengths = [len(indexes) for indexes in frame_task_indexes]
    starts = [sum(lengths[:episode]) for episode in range(len(lengths))]
    info: dict[str, Any] = {
        "codebase_version": version,
        "robot_type": "so101_follower",
        "total_episodes": len(lengths),
        "total_frames": sum(lengths),
        "total_tasks": len(TASKS),
        "chunks_size": 1000,
        "fps": FPS,
        "splits": {"train": f"0:{len(lengths)}"},
        "features": _features(camera_dtype),
    }
    (root / "meta").mkdir(parents=True)
    frames = [
        _frames(episode, starts[episode], indexes, camera_dtype)
        for episode, indexes in enumerate(frame_task_indexes)
    ]
    episode_tasks = [
        sorted({TASKS[index] for index in indexes}) for indexes in frame_task_indexes
    ]

    if version == "v3.0":
        info["data_path"] = "data/chunk-{chunk_index:03d}/file-{file_index:03d}.parquet"
        info["video_path"] = (
            "videos/{video_key}/chunk-{chunk_index:03d}/file-{file_index:03d}.mp4"
        )
        pq.write_table(
            pa.table({"task_index": list(range(len(TASKS))), "task": TASKS}),
            root / "meta" / "tasks.parquet",
        )
        _write_parquet(
            root / "data" / "chunk-000" / "file-000.parquet", pa.concat_tables(frames)
        )
        if camera_dtype == "video":
            _encode_video(
                root / "videos" / CAMERA / "chunk-000" / "file-000.mp4",
                sum(lengths),
                codec or "libsvtav1",
                gop,
                video_options,
            )
        _write_parquet(
            root / "meta" / "episodes" / "chunk-000" / "file-000.parquet",
            pa.table(
                {
                    "episode_index": list(range(len(lengths))),
                    "length": lengths,
                    "tasks": episode_tasks,
                    "data/chunk_index": [0] * len(lengths),
                    "data/file_index": [0] * len(lengths),
                    "dataset_from_index": starts,
                    "dataset_to_index": [s + n for s, n in zip(starts, lengths)],
                    f"videos/{CAMERA}/chunk_index": [0] * len(lengths),
                    f"videos/{CAMERA}/file_index": [0] * len(lengths),
                    f"videos/{CAMERA}/from_timestamp": [s / FPS for s in starts],
                    f"videos/{CAMERA}/to_timestamp": [
                        (s + n) / FPS for s, n in zip(starts, lengths)
                    ],
                }
            ),
        )
    else:
        info["data_path"] = (
            "data/chunk-{episode_chunk:03d}/episode_{episode_index:06d}.parquet"
        )
        info["video_path"] = (
            "videos/chunk-{episode_chunk:03d}/{video_key}/"
            "episode_{episode_index:06d}.mp4"
        )
        (root / "meta" / "tasks.jsonl").write_text(
            "".join(
                json.dumps({"task_index": index, "task": task}) + "\n"
                for index, task in enumerate(TASKS)
            )
        )
        (root / "meta" / "episodes.jsonl").write_text(
            "".join(
                json.dumps({"episode_index": episode, "tasks": tasks, "length": length})
                + "\n"
                for episode, (tasks, length) in enumerate(zip(episode_tasks, lengths))
            )
        )
        for episode, table in enumerate(frames):
            _write_parquet(
                root / "data" / "chunk-000" / f"episode_{episode:06d}.parquet", table
            )
            if camera_dtype == "video":
                _encode_video(
                    root
                    / "videos"
                    / "chunk-000"
                    / CAMERA
                    / f"episode_{episode:06d}.mp4",
                    lengths[episode],
                    codec or "libx264",
                    gop,
                    video_options,
                )

    (root / "meta" / "info.json").write_text(json.dumps(info, indent=2))
    return root


@pytest.fixture
def make_dataset(tmp_path: Path) -> Callable[..., Path]:
    count = 0

    def make(version: str, **options: Any) -> Path:
        nonlocal count
        count += 1
        return write_dataset(tmp_path / f"dataset{count}", version, **options)

    return make


@pytest.fixture
def v2_dataset(tmp_path: Path) -> Path:
    return write_dataset(tmp_path / "v2", "v2.1")


@pytest.fixture
def v3_dataset(tmp_path: Path) -> Path:
    return write_dataset(tmp_path / "v3", "v3.0")
