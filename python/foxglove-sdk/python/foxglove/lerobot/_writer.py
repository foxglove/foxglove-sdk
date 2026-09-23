import json
import math
from collections.abc import Iterable
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Literal

import pyarrow as pa

from .. import Channel, Context, open_mcap
from ..channels import CompressedImageChannel, CompressedVideoChannel
from ..mcap import MCAPWriter
from ..messages import CompressedImage, CompressedVideo, Timestamp
from ._dataset import INDEX_COLUMNS, Episode, Feature, FrameReader, LeRobotDataset
from ._video import read_episode_video

NS_PER_SEC = 1_000_000_000

DEFAULT_START_TIME = datetime(2020, 1, 1, tzinfo=timezone.utc)
DEFAULT_EPISODE_GAP_S = 1.0
EARLIEST_START_TIME = datetime(1970, 1, 1, tzinfo=timezone.utc)
LATEST_START_TIME = datetime(2100, 1, 1, tzinfo=timezone.utc)

LEROBOT_TIMESTAMP_TOLERANCE_S = 1e-4

TASK_TOPIC = "/task"

SCALARS_SCHEMA = {
    "type": "object",
    "title": "lerobot.Scalars",
    "properties": {
        "scalars": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "label": {"type": "string"},
                    "value": {"type": "number"},
                },
            },
        }
    },
}

TASK_SCHEMA = {
    "type": "object",
    "title": "lerobot.Task",
    "properties": {
        "task": {"type": "string"},
        "task_index": {"type": "integer"},
    },
}

TEXT_SCHEMA = {
    "type": "object",
    "title": "lerobot.Text",
    "properties": {"value": {"type": "string"}},
}

NUMERIC_DTYPES = frozenset(
    {
        "bool",
        "float16",
        "float32",
        "float64",
        "int8",
        "int16",
        "int32",
        "int64",
        "uint8",
        "uint16",
        "uint32",
        "uint64",
    }
)

TopicKind = Literal["video", "image", "scalars", "text"]


@dataclass(frozen=True)
class Topic:
    name: str
    kind: TopicKind
    features: tuple[Feature, ...]


@dataclass(frozen=True)
class WrittenEpisode:
    """An episode that :meth:`EpisodeWriter.write` wrote.

    :param path: The MCAP file.
    :param preroll_packets: The number of video frames written from before the episode's
        start, so that its first frame decodes.
    """

    path: Path
    preroll_packets: int


def plan_topics(
    dataset: LeRobotDataset,
) -> tuple[list[Topic], list[tuple[Feature, str]]]:
    topics: dict[str, Topic] = {}
    skipped: list[tuple[Feature, str]] = []
    for feature in dataset.features:
        kind: TopicKind
        if feature.key in INDEX_COLUMNS:
            continue
        if feature.is_depth_map:
            skipped.append(
                (
                    feature,
                    "depth maps are stored as quantized video and aren't supported",
                )
            )
            continue
        if feature.dtype == "video":
            kind, name = "video", camera_topic(feature.key)
        elif feature.dtype == "image":
            kind, name = "image", camera_topic(feature.key)
        elif feature.dtype in NUMERIC_DTYPES:
            kind, name = "scalars", scalars_topic(feature.key)
        elif feature.dtype == "string":
            kind, name = "text", "/" + feature.key.replace(".", "/")
        else:
            skipped.append((feature, f"dtype {feature.dtype!r} isn't supported"))
            continue

        existing = topics.get(name)
        if existing is None:
            topics[name] = Topic(name, kind, (feature,))
        elif kind == existing.kind == "scalars":
            topics[name] = Topic(name, kind, (*existing.features, feature))
        else:
            raise ValueError(
                f"features {existing.features[0].key!r} and {feature.key!r} "
                f"would both be written to {name}"
            )
    return list(topics.values()), skipped


def camera_topic(key: str) -> str:
    name = key
    for prefix in ("observation.images.", "observation."):
        if key.startswith(prefix):
            name = key[len(prefix) :]
            break
    return "/observation/images/" + name.replace(".", "_")


def scalars_topic(key: str) -> str:
    if key == "action":
        return "/action/state"
    if key.startswith("next."):
        return "/episode/state"
    return "/" + key.replace(".", "/")


def scalar_labels(feature: Feature) -> list[str]:
    if feature.names is not None:
        return list(feature.names)
    name = feature.key
    for prefix in ("observation.", "action.", "next."):
        if name.startswith(prefix):
            name = name[len(prefix) :]
            break
    size = math.prod(feature.shape)
    return [name] if size == 1 else [f"{name}_{index}" for index in range(size)]


def episode_start_times(
    episodes: Iterable[Episode], fps: float, start_time: datetime, gap_s: float
) -> dict[int, int]:
    since_epoch = start_time - datetime(1970, 1, 1, tzinfo=timezone.utc)
    cursor = (
        since_epoch.days * 86_400 + since_epoch.seconds
    ) * NS_PER_SEC + since_epoch.microseconds * 1_000
    gap_ns = round(gap_s * NS_PER_SEC)
    starts = {}
    for episode in sorted(episodes, key=lambda item: item.index):
        starts[episode.index] = cursor
        cursor += round(episode.length * NS_PER_SEC / fps) + gap_ns
    return starts


def offset_ns_snapped_to_frames(offset_s: float, fps: float) -> int:
    frame = round(offset_s * fps)
    if abs(offset_s - frame / fps) <= LEROBOT_TIMESTAMP_TOLERANCE_S:
        return round(frame * NS_PER_SEC / fps)
    return round(offset_s * NS_PER_SEC)


class EpisodeWriter:
    """Writes the episodes of a LeRobot dataset to MCAP files, one file per episode.

    LeRobot doesn't record when data was captured, so the episodes are laid end to end, in
    index order, on a timeline that starts at ``start_time``. The timeline only depends on
    the dataset and these options, so converting a dataset again reproduces the same time
    ranges.

    :param dataset: The dataset, from :func:`load_dataset`.
    :param start_time: When the dataset's first episode starts. A time without a time zone
        is taken as UTC. It has to be at or after 1970-01-01T00:00:00Z and before
        2100-01-01T00:00:00Z.
    :param episode_gap_s: Seconds between one episode's end and the next one's start. It has
        to be finite and 0 or more.
    :param strict_keyframes: Fail if an episode's video doesn't start on a keyframe, rather
        than starting it from the previous keyframe.
    :raises ValueError: If ``start_time`` or ``episode_gap_s`` is out of range, or two
        features would be written to the same topic.

    .. py:attribute:: skipped
       :type: list[tuple[Feature, str]]

       The features that aren't converted, each with the reason.
    """

    def __init__(
        self,
        dataset: LeRobotDataset,
        *,
        start_time: datetime = DEFAULT_START_TIME,
        episode_gap_s: float = DEFAULT_EPISODE_GAP_S,
        strict_keyframes: bool = False,
    ) -> None:
        utc_start_time = (
            start_time
            if start_time.tzinfo is not None
            else start_time.replace(tzinfo=timezone.utc)
        )
        if not EARLIEST_START_TIME <= utc_start_time < LATEST_START_TIME:
            raise ValueError(
                f"start time {utc_start_time.isoformat()} must be at or after "
                f"{EARLIEST_START_TIME:%Y-%m-%dT%H:%M:%SZ} and before "
                f"{LATEST_START_TIME:%Y-%m-%dT%H:%M:%SZ}"
            )
        if not math.isfinite(episode_gap_s) or episode_gap_s < 0:
            raise ValueError(
                f"episode gap {episode_gap_s} must be a finite number of seconds, "
                "0 or more"
            )

        self._dataset = dataset
        self._topics, self.skipped = plan_topics(dataset)
        self._strict_keyframes = strict_keyframes
        self._start_times = episode_start_times(
            dataset.episodes, dataset.fps, utc_start_time, episode_gap_s
        )
        self._info = (dataset.root / "meta" / "info.json").read_bytes()
        self._frames = FrameReader(
            columns=[
                feature.key
                for topic in self._topics
                if topic.kind != "video"
                for feature in topic.features
            ],
            optional_columns=["timestamp", "task_index"],
        )

        self._context = Context()
        self._channels: dict[str, Any] = {}
        for topic in self._topics:
            metadata = {"lerobot_features": ",".join(f.key for f in topic.features)}
            if topic.kind == "video":
                self._channels[topic.name] = CompressedVideoChannel(
                    topic.name, metadata=metadata, context=self._context
                )
            elif topic.kind == "image":
                self._channels[topic.name] = CompressedImageChannel(
                    topic.name, metadata=metadata, context=self._context
                )
            else:
                self._channels[topic.name] = Channel(
                    topic.name,
                    schema=SCALARS_SCHEMA if topic.kind == "scalars" else TEXT_SCHEMA,
                    message_encoding="json",
                    metadata=metadata,
                    context=self._context,
                )
        self._task_channel = Channel(
            TASK_TOPIC,
            schema=TASK_SCHEMA,
            message_encoding="json",
            context=self._context,
        )

    def write(self, episode: Episode, output_dir: str | Path) -> WrittenEpisode:
        """Write an episode to ``output_dir/episode_<index>.mcap``, with the index padded to
        six digits.

        The file is written under a temporary name and renamed once it's complete, so a
        failed or interrupted write doesn't leave a partial file behind.

        :param episode: The episode, one of the dataset's ``episodes``.
        :param output_dir: The directory to write to. It has to exist.
        :raises FileNotFoundError: If one of the episode's files is missing.
        :raises UnsupportedVideoError: If one of the episode's videos can't be written.
        :raises KeyframeError: If one of the episode's videos can't start on a keyframe.
        :raises ValueError: If the episode's frames don't match the dataset's metadata.
        """
        path = Path(output_dir) / f"episode_{episode.index:06d}.mcap"
        partial = path.with_name(path.name + ".partial")
        try:
            with open_mcap(
                partial, allow_overwrite=True, context=self._context
            ) as writer:
                preroll_packets = self._write(episode, writer)
        except BaseException:
            partial.unlink(missing_ok=True)
            raise
        partial.replace(path)
        return WrittenEpisode(path=path, preroll_packets=preroll_packets)

    def _write(self, episode: Episode, writer: MCAPWriter) -> int:
        fps = self._dataset.fps
        start_ns = self._start_times[episode.index]
        frames = self._frames.read(episode)
        if "timestamp" in frames.column_names:
            offsets = frames["timestamp"].to_pylist()
        else:
            offsets = [row / fps for row in range(frames.num_rows)]
        log_times = [
            start_ns + offset_ns_snapped_to_frames(offset, fps) for offset in offsets
        ]

        writer.write_metadata("lerobot", self._metadata(episode))
        writer.attach(
            log_time=start_ns,
            create_time=start_ns,
            name="meta/info.json",
            media_type="application/json",
            data=self._info,
        )
        self._write_tasks(episode, frames, log_times, start_ns)

        preroll_packets = 0
        for topic in self._topics:
            channel = self._channels[topic.name]
            if topic.kind == "video":
                preroll_packets += self._write_video(
                    episode, topic.features[0], channel, start_ns
                )
            elif topic.kind == "image":
                self._write_images(topic.features[0], channel, frames, log_times)
            elif topic.kind == "scalars":
                _write_scalars(topic.features, channel, frames, log_times)
            else:
                for value, log_time in zip(
                    frames[topic.features[0].key].to_pylist(), log_times
                ):
                    channel.log({"value": value}, log_time=log_time)
        return preroll_packets

    def _metadata(self, episode: Episode) -> dict[str, str]:
        metadata = {
            "dataset": self._dataset.root.name,
            "codebase_version": self._dataset.version,
            "fps": str(self._dataset.info["fps"]),
            "episode_index": str(episode.index),
            "length": str(episode.length),
            "tasks": json.dumps(list(episode.tasks)),
            "total_episodes": str(len(self._dataset.episodes)),
        }
        if self._dataset.robot_type is not None:
            metadata["robot_type"] = self._dataset.robot_type
        return metadata

    def _write_tasks(
        self, episode: Episode, frames: pa.Table, log_times: list[int], start_ns: int
    ) -> None:
        if "task_index" not in frames.column_names:
            for task in episode.tasks:
                self._task_channel.log({"task": task}, log_time=start_ns)
            return

        previous = None
        for task_index, log_time in zip(frames["task_index"].to_pylist(), log_times):
            if task_index == previous:
                continue
            previous = task_index
            self._task_channel.log(
                {"task": self._dataset.tasks.get(task_index), "task_index": task_index},
                log_time=log_time,
            )

    def _write_video(
        self,
        episode: Episode,
        feature: Feature,
        channel: CompressedVideoChannel,
        start_ns: int,
    ) -> int:
        segment = episode.videos.get(feature.key)
        if segment is None:
            raise ValueError(f"no video for {feature.key}")

        preroll_packets = 0
        for packet in read_episode_video(
            segment, self._dataset.fps, strict_keyframes=self._strict_keyframes
        ):
            log_time = start_ns + offset_ns_snapped_to_frames(
                packet.offset_s, self._dataset.fps
            )
            channel.log(
                CompressedVideo(
                    timestamp=_timestamp(log_time),
                    frame_id=feature.key,
                    data=packet.data,
                    format=packet.format,
                ),
                log_time=log_time,
            )
            preroll_packets += packet.is_preroll
        return preroll_packets

    def _write_images(
        self,
        feature: Feature,
        channel: CompressedImageChannel,
        frames: pa.Table,
        log_times: list[int],
    ) -> None:
        for value, log_time in zip(frames[feature.key].to_pylist(), log_times):
            data = _image_bytes(value)
            channel.log(
                CompressedImage(
                    timestamp=_timestamp(log_time),
                    frame_id=feature.key,
                    data=data,
                    format=_image_format(feature, data),
                ),
                log_time=log_time,
            )


def _write_scalars(
    features: tuple[Feature, ...],
    channel: Channel,
    frames: pa.Table,
    log_times: list[int],
) -> None:
    columns = [
        (feature.key, scalar_labels(feature), frames[feature.key].to_pylist())
        for feature in features
    ]
    for row, log_time in enumerate(log_times):
        scalars: list[dict[str, Any]] = []
        for key, labels, values in columns:
            elements = _flatten(values[row])
            if len(elements) != len(labels):
                raise ValueError(
                    f"{key}: frame {row} has {len(elements)} values, but the "
                    f"feature's shape has {len(labels)}"
                )
            scalars.extend(
                {"label": label, "value": _number(element)}
                for label, element in zip(labels, elements)
            )
        channel.log({"scalars": scalars}, log_time=log_time)


def _flatten(value: Any) -> list[Any]:
    if isinstance(value, list):
        return [item for element in value for item in _flatten(element)]
    return [value]


def _number(value: Any) -> float | None:
    if value is None:
        return None
    number = float(value)
    return number if math.isfinite(number) else None


def _timestamp(log_time: int) -> Timestamp:
    return Timestamp(sec=log_time // NS_PER_SEC, nsec=log_time % NS_PER_SEC)


def _image_bytes(value: Any) -> bytes:
    if isinstance(value, bytes):
        return value
    if isinstance(value, dict) and value.get("bytes"):
        return bytes(value["bytes"])
    raise ValueError(f"image isn't embedded in the data file: {value!r:.80}")


def _image_format(feature: Feature, data: bytes) -> str:
    if data.startswith(b"\x89PNG\r\n\x1a\n"):
        return "png"
    if data.startswith(b"\xff\xd8\xff"):
        return "jpeg"
    if data[:4] == b"RIFF" and data[8:12] == b"WEBP":
        return "webp"
    raise ValueError(f"{feature.key}: image isn't PNG, JPEG or WebP")
