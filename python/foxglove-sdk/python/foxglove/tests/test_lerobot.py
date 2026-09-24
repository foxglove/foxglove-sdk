import importlib
import json
import math
import sys
from collections.abc import Callable, Sequence
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

import av
import pyarrow as pa
import pyarrow.parquet as pq
import pytest
from av.video.codeccontext import VideoCodecContext
from av.video.stream import VideoStream
from foxglove.lerobot import (
    DEFAULT_START_TIME,
    EpisodeWriter,
    KeyframeError,
    UnsupportedDatasetError,
    UnsupportedVideoError,
    load_metadata,
)
from foxglove.lerobot._video import _with_sequence_header
from mcap.reader import make_reader

FPS = 10
WIDTH = 64
HEIGHT = 48
CAMERA = "observation.images.front"
VIDEO_TOPIC = "/observation/images/front"
TASKS = ["pick up the cube", "put the cube down"]
CODEC_OPTIONS = {"libsvtav1": {"preset": "12"}, "libx264": {"bf": "0"}}
SVT_LOG_ERRORS_ONLY = "1"

START_NS = int(DEFAULT_START_TIME.timestamp()) * 1_000_000_000
FRAME_NS = 100_000_000


@pytest.fixture(autouse=True)
def _quiet_svt_av1(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("SVT_LOG", SVT_LOG_ERRORS_ONLY)


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


def _frame(index: int) -> av.VideoFrame:
    level = index * 20 % 256
    pixels = bytes([level, level, level, 255]) * (WIDTH * HEIGHT)
    return av.VideoFrame.from_bytes(pixels, WIDTH, HEIGHT, format="rgba")


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
            container.mux(stream.encode(_frame(index)))
        container.mux(stream.encode(None))


def _encode_png(frame: int) -> bytes:
    codec = av.CodecContext.create("png", "w")
    assert isinstance(codec, VideoCodecContext)
    codec.width = WIDTH
    codec.height = HEIGHT
    codec.pix_fmt = "rgb24"
    return b"".join(bytes(packet) for packet in codec.encode(_frame(frame)))


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


def _write_tasks(path: Path, task_column: str) -> None:
    table = pa.table({"task_index": list(range(len(TASKS))), task_column: TASKS})
    pandas_metadata = json.dumps({"index_columns": [task_column]})
    pq.write_table(table.replace_schema_metadata({"pandas": pandas_metadata}), path)


def write_dataset(
    root: Path,
    version: str,
    *,
    frame_task_indexes: Sequence[Sequence[int]] = ([0] * 6, [0] * 4),
    camera_dtype: str = "video",
    codec: str | None = None,
    gop: int = 2,
    video_options: dict[str, str] | None = None,
    task_column: str = "task",
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
        _write_tasks(root / "meta" / "tasks.parquet", task_column)
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


@pytest.fixture(params=["v2_dataset", "v3_dataset"])
def dataset_root(request: pytest.FixtureRequest) -> Path:
    root: Path = request.getfixturevalue(request.param)
    return root


@dataclass(frozen=True)
class Image:
    timestamp_ns: int
    frame_id: str
    data: bytes
    format: str


def _varint(data: bytes, offset: int) -> tuple[int, int]:
    value = 0
    shift = 0
    while True:
        byte = data[offset]
        offset += 1
        value |= (byte & 0x7F) << shift
        shift += 7
        if not byte & 0x80:
            return value, offset


def _protobuf_fields(data: bytes) -> dict[int, Any]:
    fields: dict[int, Any] = {}
    offset = 0
    while offset < len(data):
        key, offset = _varint(data, offset)
        number, wire_type = key >> 3, key & 0x7
        if wire_type == 0:
            fields[number], offset = _varint(data, offset)
        elif wire_type == 2:
            length, offset = _varint(data, offset)
            fields[number] = data[offset : offset + length]
            offset += length
        else:
            raise ValueError(f"unexpected protobuf wire type {wire_type}")
    return fields


@dataclass(frozen=True)
class ImageFieldNumbers:
    timestamp: int
    frame_id: int
    data: int
    format: int


IMAGE_FIELD_NUMBERS = {
    "foxglove.CompressedVideo": ImageFieldNumbers(
        timestamp=1, frame_id=2, data=3, format=4
    ),
    "foxglove.CompressedImage": ImageFieldNumbers(
        timestamp=1, frame_id=4, data=2, format=3
    ),
}
TIMESTAMP_SECONDS_FIELD = 1
TIMESTAMP_NANOS_FIELD = 2


def _decode_image(schema_name: str, data: bytes) -> Image:
    numbers = IMAGE_FIELD_NUMBERS[schema_name]
    fields = _protobuf_fields(data)
    timestamp = _protobuf_fields(fields.get(numbers.timestamp, b""))
    return Image(
        timestamp_ns=timestamp.get(TIMESTAMP_SECONDS_FIELD, 0) * 1_000_000_000
        + timestamp.get(TIMESTAMP_NANOS_FIELD, 0),
        frame_id=fields.get(numbers.frame_id, b"").decode(),
        data=fields.get(numbers.data, b""),
        format=fields.get(numbers.format, b"").decode(),
    )


@dataclass
class Recording:
    schemas: dict[str, str] = field(default_factory=dict)
    messages: dict[str, list[tuple[int, Any]]] = field(default_factory=dict)
    metadata: dict[str, dict[str, str]] = field(default_factory=dict)
    attachments: dict[str, bytes] = field(default_factory=dict)

    def log_times(self, topic: str) -> list[int]:
        return [log_time for log_time, _ in self.messages[topic]]


def read_mcap(path: Path) -> Recording:
    recording = Recording()
    with path.open("rb") as file:
        reader = make_reader(file)
        for schema, channel, message in reader.iter_messages():
            assert schema is not None
            recording.schemas[channel.topic] = schema.name
            if channel.message_encoding == "json":
                decoded: Any = json.loads(message.data)
            else:
                decoded = _decode_image(schema.name, message.data)
            recording.messages.setdefault(channel.topic, []).append(
                (message.log_time, decoded)
            )
        recording.metadata = {m.name: m.metadata for m in reader.iter_metadata()}
        recording.attachments = {a.name: a.data for a in reader.iter_attachments()}
    return recording


def convert(root: Path, output: Path, **options: Any) -> list[Recording]:
    metadata = load_metadata(root)
    writer = EpisodeWriter(metadata, **options)
    output.mkdir(exist_ok=True)
    return [
        read_mcap(writer.write(episode, output).path) for episode in metadata.episodes
    ]


def decodes_standalone(video: Image) -> bool:
    decoder = {"av1": "libdav1d", "h264": "h264"}[video.format]
    codec = av.CodecContext.create(decoder, "r")
    assert isinstance(codec, VideoCodecContext)
    return bool(codec.decode(av.Packet(video.data)) or codec.decode(None))


def obu_with_size_field(obu_type: int, payload: bytes) -> bytes:
    return bytes([obu_type << 3 | 0x2, len(payload)]) + payload


def test_reports_a_missing_extra(monkeypatch: pytest.MonkeyPatch) -> None:
    for name in list(sys.modules):
        if name == "av" or name.startswith(("av.", "foxglove.lerobot")):
            monkeypatch.delitem(sys.modules, name)
    monkeypatch.setitem(sys.modules, "av", None)

    with pytest.raises(ImportError, match=r"foxglove-sdk\[lerobot\]"):
        importlib.import_module("foxglove.lerobot")


def test_writes_one_mcap_per_episode(dataset_root: Path, tmp_path: Path) -> None:
    first, second = convert(dataset_root, tmp_path / "out")

    assert first.schemas == {
        "/task": "lerobot.Task",
        VIDEO_TOPIC: "foxglove.CompressedVideo",
        "/observation/state": "lerobot.Scalars",
        "/action/state": "lerobot.Scalars",
        "/episode/state": "lerobot.Scalars",
    }
    assert {topic: len(messages) for topic, messages in first.messages.items()} == {
        "/task": 1,
        VIDEO_TOPIC: 6,
        "/observation/state": 6,
        "/action/state": 6,
        "/episode/state": 6,
    }
    assert len(second.messages["/observation/state"]) == 4
    assert sorted(path.name for path in (tmp_path / "out").iterdir()) == [
        "episode_000000.mcap",
        "episode_000001.mcap",
    ]


def test_accepts_string_paths(v2_dataset: Path, tmp_path: Path) -> None:
    metadata = load_metadata(str(v2_dataset))

    written = EpisodeWriter(metadata).write(metadata.episodes[0], str(tmp_path))

    assert written.path == tmp_path / "episode_000000.mcap"
    assert len(read_mcap(written.path).messages["/observation/state"]) == 6


def test_writes_episodes_from_several_threads_into_separate_files(
    v3_dataset: Path, tmp_path: Path
) -> None:
    metadata = load_metadata(v3_dataset)
    writer = EpisodeWriter(metadata)

    with ThreadPoolExecutor(max_workers=2) as pool:
        written = list(
            pool.map(lambda episode: writer.write(episode, tmp_path), metadata.episodes)
        )

    assert [
        len(read_mcap(episode.path).messages["/observation/state"])
        for episode in written
    ] == [6, 4]
    assert all(episode.preroll_frames == {} for episode in written)


def test_metadata_and_episodes_are_hashable(v3_dataset: Path) -> None:
    metadata = load_metadata(v3_dataset)

    assert len(set(metadata.episodes)) == 2
    assert hash(metadata) == hash(load_metadata(v3_dataset))


def test_labels_scalars_with_feature_names(v3_dataset: Path, tmp_path: Path) -> None:
    first, _ = convert(v3_dataset, tmp_path)

    _, state = first.messages["/observation/state"][1]
    assert state == {
        "scalars": [
            {"label": "shoulder.pos", "value": 1.0},
            {"label": "gripper.pos", "value": 1.5},
        ]
    }
    _, action = first.messages["/action/state"][1]
    assert [scalar["label"] for scalar in action["scalars"]] == ["shoulder", "gripper"]
    _, episode_state = first.messages["/episode/state"][-1]
    assert episode_state == {
        "scalars": [
            {"label": "reward", "value": 0.5},
            {"label": "done", "value": 1.0},
        ]
    }


def test_lays_episodes_end_to_end(dataset_root: Path, tmp_path: Path) -> None:
    first, second = convert(dataset_root, tmp_path, episode_gap_s=2.0)

    assert first.log_times("/observation/state") == [
        START_NS + frame * FRAME_NS for frame in range(6)
    ]
    second_start = START_NS + 6 * FRAME_NS + 2_000_000_000
    assert second.log_times("/observation/state") == [
        second_start + frame * FRAME_NS for frame in range(4)
    ]


def test_video_shares_log_times_with_data_and_starts_on_a_keyframe(
    dataset_root: Path, tmp_path: Path
) -> None:
    for recording in convert(dataset_root, tmp_path):
        assert recording.log_times(VIDEO_TOPIC) == recording.log_times(
            "/observation/state"
        )
        log_time, video = recording.messages[VIDEO_TOPIC][0]
        assert video.timestamp_ns == log_time
        assert video.frame_id == CAMERA
        assert decodes_standalone(video)


def test_starts_an_episode_between_keyframes_at_the_previous_keyframe(
    make_dataset: Callable[..., Path], tmp_path: Path
) -> None:
    metadata = load_metadata(make_dataset("v3.0", gop=4))

    written = EpisodeWriter(metadata).write(metadata.episodes[1], tmp_path)
    recording = read_mcap(written.path)
    start = recording.log_times("/observation/state")[0]
    assert written.preroll_frames == {CAMERA: 2}
    assert [t - start for t in recording.log_times(VIDEO_TOPIC)] == [
        frame * FRAME_NS for frame in range(-2, 4)
    ]
    assert decodes_standalone(recording.messages[VIDEO_TOPIC][0][1])

    strict = EpisodeWriter(metadata, strict_keyframes=True)
    with pytest.raises(KeyframeError):
        strict.write(metadata.episodes[1], tmp_path)


def test_rejects_b_frames_without_leaving_a_file_behind(
    make_dataset: Callable[..., Path], tmp_path: Path
) -> None:
    metadata = load_metadata(
        make_dataset(
            "v2.1",
            codec="libx264",
            gop=6,
            video_options={"bf": "2", "x264-params": "b-adapt=0"},
        )
    )

    with pytest.raises(UnsupportedVideoError, match="B-frames"):
        EpisodeWriter(metadata).write(metadata.episodes[0], tmp_path)
    assert list(tmp_path.glob("episode_*")) == []


def test_adds_the_av1_sequence_header_to_keyframes_missing_it() -> None:
    sequence_header = obu_with_size_field(1, b"\xaa")
    frame = obu_with_size_field(6, b"\xbb\xcc")
    path = Path("front.mp4")

    assert (
        _with_sequence_header(path, frame, sequence_header) == sequence_header + frame
    )
    with_header = sequence_header + frame
    assert _with_sequence_header(path, with_header, sequence_header) == with_header
    with pytest.raises(UnsupportedVideoError):
        _with_sequence_header(path, frame, b"")


def test_logs_the_task_when_it_changes(
    make_dataset: Callable[..., Path], tmp_path: Path
) -> None:
    root = make_dataset("v3.0", frame_task_indexes=[[0, 0, 0, 1, 1, 1], [1] * 4])
    first, second = convert(root, tmp_path)

    assert first.messages["/task"] == [
        (START_NS, {"task": "pick up the cube", "task_index": 0}),
        (START_NS + 3 * FRAME_NS, {"task": "put the cube down", "task_index": 1}),
    ]
    assert [task for _, task in second.messages["/task"]] == [
        {"task": "put the cube down", "task_index": 1}
    ]


@pytest.mark.parametrize("task_column", ["task", "__index_level_0__"])
def test_reads_v3_tasks_from_a_named_or_unnamed_pandas_index(
    make_dataset: Callable[..., Path], task_column: str
) -> None:
    metadata = load_metadata(make_dataset("v3.0", task_column=task_column))

    assert metadata.tasks == dict(enumerate(TASKS))


def test_rejects_a_v3_tasks_file_without_task_strings(v3_dataset: Path) -> None:
    path = v3_dataset / "meta" / "tasks.parquet"
    pq.write_table(pa.table({"task_index": [0, 1]}), path)

    with pytest.raises(UnsupportedDatasetError, match="no task column"):
        load_metadata(v3_dataset)


def test_records_episode_metadata_and_dataset_info(
    v2_dataset: Path, tmp_path: Path
) -> None:
    _, second = convert(v2_dataset, tmp_path)

    assert second.metadata["lerobot"] == {
        "dataset": "v2",
        "codebase_version": "v2.1",
        "robot_type": "so101_follower",
        "fps": "10",
        "episode_index": "1",
        "length": "4",
        "tasks": '["pick up the cube"]',
        "total_episodes": "2",
    }
    assert (
        second.attachments["meta/info.json"]
        == (v2_dataset / "meta" / "info.json").read_bytes()
    )


def test_converts_image_features_to_compressed_images(
    make_dataset: Callable[..., Path], tmp_path: Path
) -> None:
    first, _ = convert(make_dataset("v2.1", camera_dtype="image"), tmp_path)

    assert first.schemas[VIDEO_TOPIC] == "foxglove.CompressedImage"
    images = [image for _, image in first.messages[VIDEO_TOPIC]]
    assert [image.format for image in images] == ["png"] * 6
    assert all(image.data.startswith(b"\x89PNG") for image in images)


def test_writes_non_finite_values_as_null(v2_dataset: Path, tmp_path: Path) -> None:
    path = v2_dataset / "data" / "chunk-000" / "episode_000000.parquet"
    table = pq.read_table(path)
    states = table["observation.state"].to_pylist()
    states[0] = [math.nan, math.inf]
    column = table.schema.get_field_index("observation.state")
    pq.write_table(
        table.set_column(
            column,
            "observation.state",
            pa.array(states, table.schema.field(column).type),
        ),
        path,
    )

    first, _ = convert(v2_dataset, tmp_path)

    _, state = first.messages["/observation/state"][0]
    assert [scalar["value"] for scalar in state["scalars"]] == [None, None]


def test_skips_features_it_cannot_convert(v2_dataset: Path) -> None:
    info_path = v2_dataset / "meta" / "info.json"
    info = json.loads(info_path.read_text())
    info["features"]["observation.images.depth"] = {
        "dtype": "video",
        "shape": [48, 64, 1],
        "names": ["height", "width", "channels"],
        "info": {"video.is_depth_map": True},
    }
    info["features"]["language_events"] = {
        "dtype": "language",
        "shape": [1],
        "names": None,
    }
    info_path.write_text(json.dumps(info))

    writer = EpisodeWriter(load_metadata(v2_dataset))

    assert [feature.key for feature, _ in writer.skipped] == [
        "observation.images.depth",
        "language_events",
    ]


@pytest.mark.parametrize("dtype", ["string", "int64"])
def test_skips_a_task_feature_instead_of_writing_a_second_task_topic(
    v2_dataset: Path, tmp_path: Path, dtype: str
) -> None:
    info_path = v2_dataset / "meta" / "info.json"
    info = json.loads(info_path.read_text())
    info["features"]["task"] = {"dtype": dtype, "shape": [1], "names": None}
    info_path.write_text(json.dumps(info))
    metadata = load_metadata(v2_dataset)

    writer = EpisodeWriter(metadata)
    recording = read_mcap(writer.write(metadata.episodes[0], tmp_path).path)

    assert [feature.key for feature, _ in writer.skipped] == ["task"]
    assert recording.messages["/task"] == [
        (START_NS, {"task": "pick up the cube", "task_index": 0})
    ]


def test_names_the_feature_whose_values_do_not_match_its_shape(
    v2_dataset: Path, tmp_path: Path
) -> None:
    info_path = v2_dataset / "meta" / "info.json"
    info = json.loads(info_path.read_text())
    info["features"]["observation.state"]["shape"] = [3]
    info_path.write_text(json.dumps(info))

    with pytest.raises(ValueError, match="observation.state"):
        convert(v2_dataset, tmp_path)


def test_takes_start_times_without_a_time_zone_as_utc(
    v2_dataset: Path, tmp_path: Path
) -> None:
    first, _ = convert(v2_dataset, tmp_path, start_time=datetime(2021, 6, 1, 12))

    expected = datetime(2021, 6, 1, 12, tzinfo=timezone.utc)
    assert first.log_times("/observation/state")[0] == int(expected.timestamp()) * (
        1_000_000_000
    )


@pytest.mark.parametrize(
    "start_time",
    [
        datetime(1969, 12, 31, 23, 59, 59, tzinfo=timezone.utc),
        datetime(1970, 1, 1, 0, 30, tzinfo=timezone(timedelta(hours=1))),
        datetime(2100, 1, 1, tzinfo=timezone.utc),
    ],
)
def test_rejects_start_times_outside_1970_to_2100(
    v2_dataset: Path, start_time: datetime
) -> None:
    metadata = load_metadata(v2_dataset)

    with pytest.raises(ValueError, match="start time"):
        EpisodeWriter(metadata, start_time=start_time)
    EpisodeWriter(metadata, start_time=datetime(1970, 1, 1, tzinfo=timezone.utc))


@pytest.mark.parametrize("episode_gap_s", [-1.0, math.inf, math.nan])
def test_rejects_episode_gaps_that_are_negative_or_not_finite(
    v2_dataset: Path, episode_gap_s: float
) -> None:
    with pytest.raises(ValueError, match="episode gap"):
        EpisodeWriter(load_metadata(v2_dataset), episode_gap_s=episode_gap_s)


def test_rejects_datasets_it_cannot_read(tmp_path: Path) -> None:
    (tmp_path / "v1" / "meta_data").mkdir(parents=True)
    with pytest.raises(UnsupportedDatasetError, match="v1.x"):
        load_metadata(tmp_path / "v1")

    (tmp_path / "future" / "meta").mkdir(parents=True)
    (tmp_path / "future" / "meta" / "info.json").write_text(
        json.dumps({"codebase_version": "v9.0", "fps": 10, "features": {}})
    )
    with pytest.raises(UnsupportedDatasetError, match="v9.0"):
        load_metadata(tmp_path / "future")
