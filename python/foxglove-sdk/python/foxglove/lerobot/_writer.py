import base64
import json
import math
import threading
import warnings
from collections.abc import Iterator
from dataclasses import dataclass
from functools import cached_property
from pathlib import Path
from typing import Any

import pyarrow as pa
import pyarrow.parquet as pq

from .. import Channel, Context, open_mcap
from ..channels import CompressedImageChannel, CompressedVideoChannel
from ..mcap import MCAPWriter
from ..messages import CompressedImage, CompressedVideo, Timestamp
from ._dataset import (
    INDEX_COLUMNS,
    DatasetMetadata,
    Episode,
    EpisodeStatsReader,
    Feature,
    FrameReader,
)
from ._video import (
    COMPRESSED_VIDEO_FORMATS,
    BFrameWarning,
    DepthMapWarning,
    UnsupportedCodecWarning,
    VideoPacket,
    read_episode_video,
)

NS_PER_SEC = 1_000_000_000

LEROBOT_TIMESTAMP_TOLERANCE_S = 1e-4

TASK_TOPIC = "/task"
TASK_FEATURE_TOPIC = "/task_feature"

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

VALUE_SCHEMA = {"type": "object", "title": "lerobot.Value"}

VIDEO_PACKET_SCHEMA = {
    "type": "object",
    "title": "lerobot.VideoPacket",
    "properties": {
        "timestamp": {
            "type": "object",
            "properties": {"sec": {"type": "integer"}, "nsec": {"type": "integer"}},
        },
        "frame_id": {"type": "string"},
        "codec": {"type": "string"},
        "data": {"type": "string", "contentEncoding": "base64"},
        "extradata": {"type": "string", "contentEncoding": "base64"},
    },
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


class SkippedFeatureWarning(UserWarning):
    """A feature isn't written, because the data files have no column for it.
    :attr:`EpisodeWriter.skipped` lists each skipped feature with the reason.
    """


@dataclass(frozen=True)
class _WrittenVideo:
    key: str
    has_b_frames: bool
    codec: str


@dataclass(frozen=True)
class _Settings:
    context: Context
    fps: float
    data_schema: pa.Schema | None


@dataclass(frozen=True)
class _Frames:
    """An episode's frames from the data files, and when each is logged."""

    episode: Episode
    table: pa.Table
    log_times: list[int]
    start_ns: int

    def column(self, key: str) -> list[Any]:
        return list(self.table[key].to_pylist())


class _Topic:
    """A topic, the features written to it, and how an episode's messages are written.
    Channels are made when first used, so a file only lists the ones it has messages on.
    """

    schema: dict[str, Any]

    def __init__(
        self, name: str, features: tuple[Feature, ...], settings: _Settings
    ) -> None:
        self.name = name
        self.features = features
        self.feature = features[0]
        self.columns = [feature.key for feature in features]
        self._settings = settings
        self._metadata = {"lerobot_features": ",".join(self.columns)}

    @property
    def required_columns(self) -> list[str]:
        return self.columns

    @property
    def optional_columns(self) -> list[str]:
        return []

    def write(self, frames: _Frames) -> _WrittenVideo | None:
        raise NotImplementedError

    @cached_property
    def _channel(self) -> Any:
        return self._json_channel(self.schema)

    def _json_channel(self, schema: dict[str, Any]) -> Channel:
        return Channel(
            self.name,
            schema=schema,
            message_encoding="json",
            metadata=self._metadata,
            context=self._settings.context,
        )


class _ScalarsTopic(_Topic):
    schema = SCALARS_SCHEMA

    def write(self, frames: _Frames) -> None:
        columns = [
            (feature.key, scalar_labels(feature), frames.column(feature.key))
            for feature in self.features
        ]
        for row, log_time in enumerate(frames.log_times):
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
            self._channel.log({"scalars": scalars}, log_time=log_time)


class _ValueTopic(_Topic):
    def __init__(
        self, name: str, features: tuple[Feature, ...], settings: _Settings
    ) -> None:
        super().__init__(name, features, settings)
        data_schema = settings.data_schema
        self._value_schema = (
            _json_schema(data_schema.field(self.feature.key).type)
            if data_schema is not None and self.feature.key in data_schema.names
            else None
        )

    @property
    def required_columns(self) -> list[str]:
        return []

    @property
    def optional_columns(self) -> list[str]:
        return self.columns

    def write(self, frames: _Frames) -> None:
        if self.feature.key in frames.table.column_names:
            for value, log_time in zip(
                frames.column(self.feature.key), frames.log_times
            ):
                json_value = _json_value(value)
                if self._value_schema is None:
                    json_value = json.dumps(json_value)
                self._channel.log({"value": json_value}, log_time=log_time)

    @cached_property
    def _channel(self) -> Any:
        return self._json_channel(
            {
                **VALUE_SCHEMA,
                "properties": {"value": self._value_schema or {"type": "string"}},
            }
        )


class _ImageTopic(_Topic):
    def write(self, frames: _Frames) -> None:
        for value, log_time in zip(frames.column(self.feature.key), frames.log_times):
            data = _image_bytes(value)
            self._channel.log(
                CompressedImage(
                    timestamp=_timestamp(log_time),
                    frame_id=self.feature.key,
                    data=data,
                    format=_image_format(self.feature, data),
                ),
                log_time=log_time,
            )

    @cached_property
    def _channel(self) -> Any:
        return CompressedImageChannel(
            self.name, metadata=self._metadata, context=self._settings.context
        )


class _VideoTopic(_Topic):
    """Frames go to a ``foxglove.CompressedVideo`` channel, or a ``lerobot.VideoPacket`` one
    for other codecs."""

    @property
    def required_columns(self) -> list[str]:
        return []

    def earliest_ns(self, episode: Episode) -> int:
        """How far from the episode's start its earliest frame is shown or decoded."""
        first = next(self._read(episode, with_data=False), None)
        if first is None:
            return 0
        return min(
            self._offset_ns(first.offset_s), self._offset_ns(first.decode_offset_s)
        )

    def write(self, frames: _Frames) -> _WrittenVideo:
        key = self.feature.key
        has_b_frames = False
        codec = ""
        for packet in self._read(frames.episode):
            shown = frames.start_ns + self._offset_ns(packet.offset_s)
            decoded = frames.start_ns + self._offset_ns(packet.decode_offset_s)
            codec = packet.format
            if codec in COMPRESSED_VIDEO_FORMATS:
                self._channel.log(
                    CompressedVideo(
                        timestamp=_timestamp(shown),
                        frame_id=key,
                        data=packet.data,
                        format=codec,
                    ),
                    log_time=decoded,
                )
            else:
                message = {
                    "timestamp": _timestamp_fields(shown),
                    "frame_id": key,
                    "codec": codec,
                    "data": _base64(packet.data),
                }
                if packet.extradata:
                    message["extradata"] = _base64(packet.extradata)
                self._packet_channel.log(message, log_time=decoded)
            has_b_frames = has_b_frames or packet.is_b_frame
        return _WrittenVideo(key, has_b_frames, codec)

    @cached_property
    def _channel(self) -> Any:
        return CompressedVideoChannel(
            self.name, metadata=self._metadata, context=self._settings.context
        )

    @cached_property
    def _packet_channel(self) -> Channel:
        return self._json_channel(VIDEO_PACKET_SCHEMA)

    def _read(
        self, episode: Episode, *, with_data: bool = True
    ) -> Iterator[VideoPacket]:
        segment = episode.videos.get(self.feature.key)
        if segment is None:
            raise ValueError(f"no video for {self.feature.key}")
        return read_episode_video(
            segment,
            self._settings.fps,
            with_data=with_data,
        )

    def _offset_ns(self, offset_s: float) -> int:
        return offset_ns_snapped_to_frames(offset_s, self._settings.fps)


def plan_topics(
    metadata: DatasetMetadata, data_schema: pa.Schema | None = None
) -> tuple[
    dict[str, tuple[type[_Topic], tuple[Feature, ...]]], list[tuple[Feature, str]]
]:
    topics: dict[str, tuple[type[_Topic], tuple[Feature, ...]]] = {}
    skipped: list[tuple[Feature, str]] = []
    for feature in metadata.features:
        kind: type[_Topic]
        if feature.key in INDEX_COLUMNS:
            continue
        if (
            feature.dtype != "video"
            and data_schema is not None
            and feature.key not in data_schema.names
        ):
            skipped.append((feature, "the data files have no column for it"))
            continue
        if feature.dtype == "video":
            kind, name = _VideoTopic, camera_topic(feature.key)
        elif feature.dtype == "image":
            kind, name = _ImageTopic, camera_topic(feature.key)
        elif feature.dtype in NUMERIC_DTYPES and all(
            isinstance(size, int) for size in feature.shape
        ):
            kind, name = _ScalarsTopic, scalars_topic(feature.key)
        else:
            kind, name = _ValueTopic, _feature_topic(feature.key)
        if name == TASK_TOPIC:
            name = TASK_FEATURE_TOPIC

        existing = topics.get(name)
        if existing is None:
            topics[name] = (kind, (feature,))
        elif kind is existing[0] is _ScalarsTopic:
            topics[name] = (kind, (*existing[1], feature))
        else:
            raise ValueError(
                f"features {existing[1][0].key!r} and {feature.key!r} "
                f"would both be written to {name}"
            )
    return topics, skipped


def camera_topic(key: str) -> str:
    name = _without_prefix(key, ("observation.images.", "observation."))
    return "/observation/images/" + name.replace(".", "_")


def scalars_topic(key: str) -> str:
    if key == "action":
        return "/action/state"
    if key.startswith("next."):
        return "/episode/state"
    return _feature_topic(key)


def scalar_labels(feature: Feature) -> list[str]:
    if feature.names is not None:
        return list(feature.names)
    name = _without_prefix(feature.key, ("observation.", "action.", "next."))
    dims = [dim for dim in feature.shape if dim is not None]
    if len(dims) != len(feature.shape):
        raise ValueError(
            f"{feature.key}: shape {feature.shape} has a variable-length dimension"
        )
    size = math.prod(dims)
    return [name] if size == 1 else [f"{name}_{index}" for index in range(size)]


def offset_ns_snapped_to_frames(offset_s: float, fps: float) -> int:
    frame = round(offset_s * fps)
    if abs(offset_s - frame / fps) <= LEROBOT_TIMESTAMP_TOLERANCE_S:
        return round(frame * NS_PER_SEC / fps)
    return round(offset_s * NS_PER_SEC)


class EpisodeWriter:
    """Writes the episodes of a LeRobot dataset to MCAP files, one file per episode.

    LeRobot doesn't record when data was captured, so each episode starts at time zero,
    1970-01-01T00:00:00Z, the earliest time an MCAP file can hold, and its log times are its
    own timestamps. An episode whose video has frames from before its start, back to the
    keyframe decoding starts from, starts just late enough that the earliest of them is at
    time zero.

    Depth map videos are written like other videos, as the 12-bit codes LeRobot quantizes
    depth to, and the writer warns about each with :class:`DepthMapWarning`.

    ``write`` calls run one at a time, so calling it from several threads gains nothing. To
    write episodes in parallel, use one ``EpisodeWriter`` per thread or process.

    :param metadata: The dataset's metadata, from :func:`load_metadata`.
    :raises ValueError: If two features would be written to the same topic.

    .. py:attribute:: skipped
       :type: list[tuple[Feature, str]]

       The features that aren't converted, each with the reason. The writer warns about each
       with :class:`SkippedFeatureWarning`.
    """

    def __init__(self, metadata: DatasetMetadata) -> None:
        self._metadata = metadata
        self._lock = threading.Lock()
        self._context = Context()
        data_schema = _data_schema(metadata)
        settings = _Settings(self._context, metadata.fps, data_schema)
        planned, self.skipped = plan_topics(metadata, data_schema)
        for feature, reason in self.skipped:
            warnings.warn(
                f"{feature.key} is skipped: {reason}.",
                SkippedFeatureWarning,
                stacklevel=2,
            )
        self._topics = [
            kind(name, features, settings) for name, (kind, features) in planned.items()
        ]
        self._frames = FrameReader(
            [column for topic in self._topics for column in topic.required_columns],
            optional_columns=[
                "timestamp",
                "task_index",
                *(
                    column
                    for topic in self._topics
                    for column in topic.optional_columns
                ),
            ],
        )
        self._video_topics = [
            topic for topic in self._topics if isinstance(topic, _VideoTopic)
        ]
        for topic in self._video_topics:
            if topic.feature.is_depth_map:
                warnings.warn(
                    f"{topic.feature.key} is a depth map. Its frames are written as they "
                    "are, as LeRobot's 12-bit quantized codes, so Foxglove shows the codes "
                    "rather than depth.",
                    DepthMapWarning,
                    stacklevel=2,
                )
        self._info = (metadata.root / "meta" / "info.json").read_bytes()
        stats_path = metadata.root / "meta" / "stats.json"
        self._stats = stats_path.read_bytes() if stats_path.exists() else None
        self._episode_stats = EpisodeStatsReader(metadata)
        self._task_channel = Channel(
            TASK_TOPIC,
            schema=TASK_SCHEMA,
            message_encoding="json",
            context=self._context,
        )

    def write(self, episode: Episode, output_dir: str | Path) -> Path:
        """Write an episode to ``output_dir/episode_<index>.mcap``, with the index padded to
        six digits.

        The file is written under a temporary name and renamed once it's complete, so a
        failed or interrupted write doesn't leave a partial file behind. An existing file at
        that path is replaced. For each video with
        B-frames, which Foxglove can't play back, it warns with :class:`BFrameWarning`, and
        for each video in a codec ``foxglove.CompressedVideo`` can't hold, with
        :class:`UnsupportedCodecWarning`.

        :param episode: The episode, one of the metadata's ``episodes``.
        :param output_dir: The directory to write to. It has to exist.
        :returns: The file's path.
        :raises FileNotFoundError: If one of the episode's files is missing.
        :raises UnsupportedVideoError: If one of the episode's videos can't be read.
        :raises ValueError: If the episode's frames don't match the dataset's metadata.
        """
        path = Path(output_dir) / f"episode_{episode.index:06d}.mcap"
        partial = path.with_name(path.name + ".partial")
        with self._lock:
            try:
                with open_mcap(
                    partial, allow_overwrite=True, context=self._context
                ) as writer:
                    videos = self._write(episode, writer)
            except BaseException:
                partial.unlink(missing_ok=True)
                raise
            partial.replace(path)
        for video in videos:
            if video.has_b_frames:
                warnings.warn(
                    f"{video.key} has B-frames, which Foxglove can't play back. Its frames "
                    "are written as they are, in decode order. To view it, re-encode it "
                    "without B-frames, e.g. with ffmpeg's -bf 0.",
                    BFrameWarning,
                    stacklevel=2,
                )
            if video.codec not in COMPRESSED_VIDEO_FORMATS:
                warnings.warn(
                    f"{video.key} is {video.codec} video, which foxglove.CompressedVideo "
                    "can't hold. Its frames are written as lerobot.VideoPacket messages, "
                    "which Foxglove can't play back.",
                    UnsupportedCodecWarning,
                    stacklevel=2,
                )
        return path

    def _write(self, episode: Episode, writer: MCAPWriter) -> list[_WrittenVideo]:
        fps = self._metadata.fps
        # Time zero, or late enough that none of the episode's video frames is before it.
        start_ns = -min(
            [0, *(topic.earliest_ns(episode) for topic in self._video_topics)]
        )
        frames = self._frames.read(episode)
        if "timestamp" in frames.column_names:
            offsets = frames["timestamp"].to_pylist()
        else:
            offsets = [row / fps for row in range(frames.num_rows)]
        log_times = [
            start_ns + offset_ns_snapped_to_frames(offset, fps) for offset in offsets
        ]

        writer.write_metadata("lerobot", self._metadata_record(episode))
        episode_stats = self._episode_stats.read(episode.index)
        attachments = {
            "meta/info.json": self._info,
            "meta/stats.json": self._stats,
            "episode_stats.json": (
                None
                if episode_stats is None
                else json.dumps(_json_value(episode_stats)).encode()
            ),
        }
        for name, data in attachments.items():
            if data is not None:
                writer.attach(
                    log_time=start_ns,
                    create_time=start_ns,
                    name=name,
                    media_type="application/json",
                    data=data,
                )
        self._write_tasks(episode, frames, log_times, start_ns)

        episode_frames = _Frames(episode, frames, log_times, start_ns)
        written = [topic.write(episode_frames) for topic in self._topics]
        return [video for video in written if video is not None]

    def _metadata_record(self, episode: Episode) -> dict[str, str]:
        metadata = {
            "dataset": _dataset_name(self._metadata.root),
            "codebase_version": self._metadata.version,
            "fps": str(self._metadata.info["fps"]),
            "episode_index": str(episode.index),
            "length": str(episode.length),
            "tasks": json.dumps(list(episode.tasks)),
            "total_episodes": str(len(self._metadata.episodes)),
        }
        if self._metadata.robot_type is not None:
            metadata["robot_type"] = self._metadata.robot_type
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
                {
                    "task": self._metadata.tasks.get(task_index),
                    "task_index": task_index,
                },
                log_time=log_time,
            )


def _dataset_name(root: Path) -> str:
    path = root.resolve()
    if path.parent.name == "snapshots" and path.parent.parent.name.startswith(
        "datasets--"
    ):
        return path.parent.parent.name.removeprefix("datasets--").replace("--", "/")
    return path.name


def _data_schema(metadata: DatasetMetadata) -> pa.Schema | None:
    for episode in metadata.episodes:
        if episode.data_path.exists():
            return pq.read_schema(episode.data_path)
    return None


def _json_schema(arrow_type: pa.DataType) -> dict[str, Any] | None:
    if pa.types.is_boolean(arrow_type):
        return {"type": "boolean"}
    if pa.types.is_integer(arrow_type):
        return {"type": "integer"}
    if pa.types.is_floating(arrow_type):
        return {"type": "number"}
    if pa.types.is_string(arrow_type) or pa.types.is_large_string(arrow_type):
        return {"type": "string"}
    if (
        pa.types.is_binary(arrow_type)
        or pa.types.is_large_binary(arrow_type)
        or pa.types.is_fixed_size_binary(arrow_type)
    ):
        return {"type": "string", "contentEncoding": "base64"}
    if (
        pa.types.is_list(arrow_type)
        or pa.types.is_large_list(arrow_type)
        or pa.types.is_fixed_size_list(arrow_type)
    ):
        items = _json_schema(arrow_type.value_type)
        if items is None or items["type"] == "array":
            return None
        return {"type": "array", "items": items}
    if pa.types.is_struct(arrow_type):
        properties = {}
        for arrow_field in arrow_type:
            field_schema = _json_schema(arrow_field.type)
            if field_schema is None:
                return None
            properties[arrow_field.name] = field_schema
        return {"type": "object", "properties": properties}
    if pa.types.is_dictionary(arrow_type):
        return _json_schema(arrow_type.value_type)
    if (
        pa.types.is_timestamp(arrow_type)
        or pa.types.is_date(arrow_type)
        or pa.types.is_time(arrow_type)
        or pa.types.is_duration(arrow_type)
        or pa.types.is_decimal(arrow_type)
    ):
        return {"type": "string"}
    return None


def _json_value(value: Any) -> Any:
    if value is None or isinstance(value, (bool, int, str)):
        return value
    if isinstance(value, float):
        return _finite(value)
    if isinstance(value, (bytes, bytearray)):
        return _base64(value)
    if isinstance(value, dict):
        return {str(key): _json_value(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_json_value(item) for item in value]
    return str(value)


def _feature_topic(key: str) -> str:
    return "/" + key.replace(".", "/")


def _without_prefix(key: str, prefixes: tuple[str, ...]) -> str:
    for prefix in prefixes:
        if key.startswith(prefix):
            return key[len(prefix) :]
    return key


def _flatten(value: Any) -> list[Any]:
    if isinstance(value, list):
        return [item for element in value for item in _flatten(element)]
    return [value]


def _number(value: Any) -> float | None:
    return None if value is None else _finite(float(value))


def _finite(number: float) -> float | None:
    return number if math.isfinite(number) else None


def _base64(data: bytes | bytearray) -> str:
    return base64.b64encode(data).decode()


def _timestamp_fields(time_ns: int) -> dict[str, int]:
    sec, nsec = divmod(time_ns, NS_PER_SEC)
    return {"sec": sec, "nsec": nsec}


def _timestamp(time_ns: int) -> Timestamp:
    return Timestamp(**_timestamp_fields(time_ns))


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
