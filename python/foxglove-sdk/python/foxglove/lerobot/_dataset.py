import json
import math
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import pyarrow as pa
import pyarrow.compute as pc
import pyarrow.parquet as pq

SUPPORTED_VERSIONS = ("v2.0", "v2.1", "v3.0")

INDEX_COLUMNS = frozenset(
    {"timestamp", "frame_index", "episode_index", "index", "task_index"}
)

VIDEO_COLUMNS = ("chunk_index", "file_index", "from_timestamp", "to_timestamp")


class UnsupportedDatasetError(Exception):
    """The directory doesn't hold a LeRobot dataset in a supported format."""


@dataclass(frozen=True)
class Feature:
    """A feature from the dataset's ``meta/info.json``.

    :param key: The feature's key, such as ``observation.state``.
    :param dtype: The feature's dtype, such as ``float32`` or ``video``.
    :param shape: The feature's shape.
    :param names: One name per element, or None if the dataset doesn't give exactly one.
    :param is_depth_map: Whether the feature is a depth map video.
    """

    key: str
    dtype: str
    shape: tuple[int, ...]
    names: tuple[str, ...] | None
    is_depth_map: bool


@dataclass(frozen=True)
class VideoSegment:
    """The part of an mp4 that holds one episode's video.

    :param path: The mp4's path.
    :param start_s: The episode's start in the mp4, in seconds.
    :param end_s: The episode's end in the mp4, in seconds.
    """

    path: Path
    start_s: float
    end_s: float


@dataclass(frozen=True)
class Episode:
    """An episode of a dataset.

    :param index: The episode's index.
    :param length: The episode's number of frames.
    :param tasks: The episode's tasks.
    :param data_path: The parquet file that holds the episode's frames.
    :param videos: The episode's video for each video feature, by feature key.
    """

    index: int
    length: int
    tasks: tuple[str, ...]
    data_path: Path
    videos: dict[str, VideoSegment]


@dataclass(frozen=True)
class LeRobotDataset:
    """A LeRobot dataset, as described by its ``meta`` directory. Use :func:`load_dataset`
    to load one.

    :param root: The dataset's root directory.
    :param info: The contents of ``meta/info.json``.
    :param features: The dataset's features.
    :param episodes: The dataset's episodes, in index order.
    :param tasks: The dataset's tasks, by task index.
    """

    root: Path
    info: dict[str, Any]
    features: tuple[Feature, ...]
    episodes: tuple[Episode, ...]
    tasks: dict[int, str]

    @property
    def version(self) -> str:
        """The dataset's codebase version, such as ``v3.0``."""
        return str(self.info["codebase_version"])

    @property
    def fps(self) -> float:
        """The dataset's frame rate."""
        return float(self.info["fps"])

    @property
    def robot_type(self) -> str | None:
        """The dataset's robot type, if it has one."""
        robot_type = self.info.get("robot_type")
        return None if robot_type is None else str(robot_type)


def load_dataset(root: Path) -> LeRobotDataset:
    """Load the metadata of a LeRobot dataset in the v2.0, v2.1 or v3.0 format.

    Frames and videos are read later, one episode at a time, by :class:`EpisodeWriter`.

    :param root: The dataset's root directory, the one holding ``meta/``, ``data/`` and
        ``videos/``.
    :raises UnsupportedDatasetError: If the directory doesn't hold a dataset in a
        supported format.
    """
    info_path = root / "meta" / "info.json"
    if not info_path.exists():
        if (root / "meta_data").exists():
            raise UnsupportedDatasetError(
                f"{root} is a LeRobot v1.x dataset, which predates the documented format. "
                "Convert it to v2.0 with LeRobot's v1 to v2 conversion script first."
            )
        raise UnsupportedDatasetError(f"{root} has no meta/info.json")

    info = json.loads(info_path.read_text())
    version = info.get("codebase_version")
    if version not in SUPPORTED_VERSIONS:
        raise UnsupportedDatasetError(
            f"codebase_version {version!r} is not supported "
            f"(supported: {', '.join(SUPPORTED_VERSIONS)})"
        )

    features = tuple(
        Feature(
            key=key,
            dtype=spec["dtype"],
            shape=tuple(spec.get("shape") or ()),
            names=_names(spec),
            is_depth_map=bool((spec.get("info") or {}).get("video.is_depth_map")),
        )
        for key, spec in info["features"].items()
    )
    video_keys = [feature.key for feature in features if feature.dtype == "video"]
    if version == "v3.0":
        episodes = _episodes_v3(root, info, video_keys)
    else:
        episodes = _episodes_v2(root, info, video_keys)

    return LeRobotDataset(
        root=root,
        info=info,
        features=features,
        episodes=episodes,
        tasks=_tasks(root),
    )


def _names(spec: dict[str, Any]) -> tuple[str, ...] | None:
    names = spec.get("names")
    labels: list[str] | None = None
    if isinstance(names, dict):
        groups = list(names.values())
        if groups and all(isinstance(group, list) for group in groups):
            labels = [str(name) for group in groups for name in group]
        elif groups and all(
            isinstance(index, int) and not isinstance(index, bool) for index in groups
        ):
            labels = [str(name) for name in sorted(names, key=names.__getitem__)]
    elif isinstance(names, list):
        labels = [str(name) for name in names]

    if labels is None or len(labels) != math.prod(spec.get("shape") or ()):
        return None
    return tuple(labels)


def _read_jsonl(path: Path) -> list[dict[str, Any]]:
    with path.open() as lines:
        return [json.loads(line) for line in lines if line.strip()]


def _episodes_v2(
    root: Path, info: dict[str, Any], video_keys: list[str]
) -> tuple[Episode, ...]:
    path = root / "meta" / "episodes.jsonl"
    if not path.exists():
        raise UnsupportedDatasetError(f"{root} has no meta/episodes.jsonl")

    fps = float(info["fps"])
    chunks_size = int(info.get("chunks_size", 1000))
    episodes = []
    for entry in _read_jsonl(path):
        index = int(entry["episode_index"])
        length = int(entry["length"])
        chunk = index // chunks_size
        episodes.append(
            Episode(
                index=index,
                length=length,
                tasks=tuple(entry.get("tasks") or ()),
                data_path=root
                / info["data_path"].format(episode_chunk=chunk, episode_index=index),
                videos={
                    key: VideoSegment(
                        path=root
                        / info["video_path"].format(
                            episode_chunk=chunk, video_key=key, episode_index=index
                        ),
                        start_s=0.0,
                        end_s=length / fps,
                    )
                    for key in video_keys
                },
            )
        )
    return tuple(sorted(episodes, key=lambda episode: episode.index))


def _episodes_v3(
    root: Path, info: dict[str, Any], video_keys: list[str]
) -> tuple[Episode, ...]:
    paths = sorted((root / "meta" / "episodes").rglob("*.parquet"))
    if not paths:
        raise UnsupportedDatasetError(f"{root} has no meta/episodes/*.parquet")

    wanted = [
        "episode_index",
        "length",
        "tasks",
        "data/chunk_index",
        "data/file_index",
        *(f"videos/{key}/{column}" for key in video_keys for column in VIDEO_COLUMNS),
    ]
    episodes = []
    for path in paths:
        available = set(pq.read_schema(path).names)
        table = pq.read_table(
            path, columns=[name for name in wanted if name in available]
        )
        for row in table.to_pylist():
            videos = {}
            for key in video_keys:
                prefix = f"videos/{key}/"
                if row.get(prefix + "file_index") is None:
                    continue
                videos[key] = VideoSegment(
                    path=root
                    / info["video_path"].format(
                        video_key=key,
                        chunk_index=int(row[prefix + "chunk_index"]),
                        file_index=int(row[prefix + "file_index"]),
                    ),
                    start_s=float(row[prefix + "from_timestamp"]),
                    end_s=float(row[prefix + "to_timestamp"]),
                )
            episodes.append(
                Episode(
                    index=int(row["episode_index"]),
                    length=int(row["length"]),
                    tasks=tuple(row.get("tasks") or ()),
                    data_path=root
                    / info["data_path"].format(
                        chunk_index=int(row["data/chunk_index"]),
                        file_index=int(row["data/file_index"]),
                    ),
                    videos=videos,
                )
            )
    return tuple(sorted(episodes, key=lambda episode: episode.index))


def _tasks(root: Path) -> dict[int, str]:
    jsonl = root / "meta" / "tasks.jsonl"
    if jsonl.exists():
        return {int(row["task_index"]): str(row["task"]) for row in _read_jsonl(jsonl)}

    parquet = root / "meta" / "tasks.parquet"
    if not parquet.exists():
        return {}
    table = pq.read_table(parquet, columns=["task_index", "task"])
    return {
        int(index): str(task)
        for index, task in zip(
            table["task_index"].to_pylist(), table["task"].to_pylist()
        )
    }


class FrameReader:
    def __init__(self, columns: list[str], optional_columns: list[str]) -> None:
        self._columns = columns
        self._optional_columns = optional_columns
        self._path: Path | None = None
        self._table: pa.Table | None = None

    def read(self, episode: Episode) -> pa.Table:
        if self._table is None or episode.data_path != self._path:
            self._table = self._read_file(episode.data_path)
            self._path = episode.data_path

        frames = self._table.filter(
            pc.equal(self._table["episode_index"], episode.index)
        )
        if frames.num_rows != episode.length:
            raise ValueError(
                f"{episode.data_path.name} holds {frames.num_rows} frames for the "
                f"episode, but meta/episodes gives a length of {episode.length}"
            )
        return frames

    def _read_file(self, path: Path) -> pa.Table:
        if not path.exists():
            raise FileNotFoundError(f"missing data file {path}")
        available = set(pq.read_schema(path).names)
        missing = [
            name for name in ["episode_index", *self._columns] if name not in available
        ]
        if missing:
            raise ValueError(f"{path} has no column(s) {', '.join(missing)}")
        optional = [name for name in self._optional_columns if name in available]
        columns = list(dict.fromkeys(["episode_index", *self._columns, *optional]))
        return pq.read_table(path, columns=columns)
