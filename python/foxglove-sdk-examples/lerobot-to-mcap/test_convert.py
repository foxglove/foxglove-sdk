import json
import math
from argparse import ArgumentTypeError
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import av
import pyarrow as pa
import pyarrow.parquet as pq
import pytest
from av.video.codeccontext import VideoCodecContext
from convert import DEFAULT_START_TIME, EpisodeWriter, plan_topics
from lerobot_dataset import UnsupportedDatasetError, load_dataset
from main import parse_episodes
from mcap.reader import make_reader
from mcap_protobuf.decoder import DecoderFactory
from video import KeyframeError, UnsupportedVideoError, _with_sequence_header

START_NS = int(DEFAULT_START_TIME.timestamp()) * 1_000_000_000
FRAME_NS = 100_000_000
VIDEO_TOPIC = "/observation/images/front"


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
    decoders: dict[int, Callable[[bytes], Any]] = {}
    with path.open("rb") as file:
        reader = make_reader(file)
        for schema, channel, message in reader.iter_messages():
            assert schema is not None
            recording.schemas[channel.topic] = schema.name
            if channel.message_encoding == "json":
                decoded = json.loads(message.data)
            else:
                if schema.id not in decoders:
                    decoder = DecoderFactory().decoder_for("protobuf", schema)
                    assert decoder is not None
                    decoders[schema.id] = decoder
                decoded = decoders[schema.id](message.data)
            recording.messages.setdefault(channel.topic, []).append(
                (message.log_time, decoded)
            )
        recording.metadata = {m.name: m.metadata for m in reader.iter_metadata()}
        recording.attachments = {a.name: a.data for a in reader.iter_attachments()}
    return recording


def convert(root: Path, output: Path, **options: Any) -> list[Recording]:
    dataset = load_dataset(root)
    writer = EpisodeWriter(dataset, **options)
    output.mkdir(exist_ok=True)
    return [
        read_mcap(writer.write(episode, output).path) for episode in dataset.episodes
    ]


def decodes_standalone(video: Any) -> bool:
    decoder = {"av1": "libdav1d", "h264": "h264"}[video.format]
    codec = av.CodecContext.create(decoder, "r")
    assert isinstance(codec, VideoCodecContext)
    return bool(codec.decode(av.Packet(video.data)) or codec.decode(None))


@pytest.fixture(params=["v2_dataset", "v3_dataset"])
def dataset_root(request: pytest.FixtureRequest) -> Path:
    root: Path = request.getfixturevalue(request.param)
    return root


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
        assert video.timestamp.ToNanoseconds() == log_time
        assert decodes_standalone(video)


def test_starts_an_episode_between_keyframes_at_the_previous_keyframe(
    make_dataset: Callable[..., Path], tmp_path: Path
) -> None:
    dataset = load_dataset(make_dataset("v3.0", gop=4))

    written = EpisodeWriter(dataset).write(dataset.episodes[1], tmp_path)
    recording = read_mcap(written.path)
    start = recording.log_times("/observation/state")[0]
    assert written.preroll_packets == 2
    assert [t - start for t in recording.log_times(VIDEO_TOPIC)] == [
        frame * FRAME_NS for frame in range(-2, 4)
    ]
    assert decodes_standalone(recording.messages[VIDEO_TOPIC][0][1])

    strict = EpisodeWriter(dataset, strict_keyframes=True)
    with pytest.raises(KeyframeError):
        strict.write(dataset.episodes[1], tmp_path)


def test_rejects_b_frames_without_leaving_a_file_behind(
    make_dataset: Callable[..., Path], tmp_path: Path
) -> None:
    dataset = load_dataset(
        make_dataset("v2.1", codec="libx264", gop=6, video_options={"bf": "2"})
    )

    with pytest.raises(UnsupportedVideoError, match="B-frames"):
        EpisodeWriter(dataset).write(dataset.episodes[0], tmp_path)
    assert list(tmp_path.glob("episode_*")) == []


def obu_with_size_field(obu_type: int, payload: bytes) -> bytes:
    return bytes([obu_type << 3 | 0x2, len(payload)]) + payload


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

    topics, skipped = plan_topics(load_dataset(v2_dataset))

    assert [feature.key for feature, _ in skipped] == [
        "observation.images.depth",
        "language_events",
    ]
    assert "/observation/images/depth" not in {topic.name for topic in topics}


def test_rejects_datasets_it_cannot_read(tmp_path: Path) -> None:
    (tmp_path / "v1" / "meta_data").mkdir(parents=True)
    with pytest.raises(UnsupportedDatasetError, match="v1.x"):
        load_dataset(tmp_path / "v1")

    (tmp_path / "future" / "meta").mkdir(parents=True)
    (tmp_path / "future" / "meta" / "info.json").write_text(
        json.dumps({"codebase_version": "v9.0", "fps": 10, "features": {}})
    )
    with pytest.raises(UnsupportedDatasetError, match="v9.0"):
        load_dataset(tmp_path / "future")


@pytest.mark.parametrize(
    ("spec", "expected"),
    [("3", {3}), ("0,2", {0, 2}), ("1-3", {1, 2, 3}), (" 0, 5-7 ,9", {0, 5, 6, 7, 9})],
)
def test_parses_episode_selections(spec: str, expected: set[int]) -> None:
    assert parse_episodes(spec) == expected


@pytest.mark.parametrize("spec", ["1-x", "5-3", "", " , "])
def test_rejects_malformed_or_empty_episode_selections(spec: str) -> None:
    with pytest.raises(ArgumentTypeError):
        parse_episodes(spec)


def test_names_the_feature_whose_values_do_not_match_its_shape(
    v2_dataset: Path, tmp_path: Path
) -> None:
    info_path = v2_dataset / "meta" / "info.json"
    info = json.loads(info_path.read_text())
    info["features"]["observation.state"]["shape"] = [3]
    info_path.write_text(json.dumps(info))

    with pytest.raises(ValueError, match="observation.state"):
        convert(v2_dataset, tmp_path)
