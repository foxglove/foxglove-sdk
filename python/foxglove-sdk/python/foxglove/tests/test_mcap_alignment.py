from __future__ import annotations

import io
import subprocess
import sys
from fractions import Fraction
from pathlib import Path

import pytest

try:
    import av
    from google.protobuf import descriptor_pb2, descriptor_pool, message_factory
    from google.protobuf.message import Message
    from google.protobuf.timestamp_pb2 import DESCRIPTOR as TIMESTAMP_DESCRIPTOR
    from google.protobuf.wrappers_pb2 import Int32Value
    from mcap_protobuf.decoder import DecoderFactory
    from mcap_protobuf.writer import Writer as ProtobufWriter
    from PIL import Image
except ImportError:
    has_video_dependencies = False
else:
    has_video_dependencies = True

from mcap.reader import make_reader
from mcap.writer import CompressionType, IndexType, Writer

MILLISECOND = 1_000_000
OUTPUT_NAME = "aligned.mcap"


def write_source(
    path: Path, samples: dict[str, list[tuple[int, int]]], *, indexed: bool = True
) -> Path:
    with path.open("wb") as handle:
        writer = Writer(
            handle,
            compression=CompressionType.NONE,
            index_types=IndexType.ALL if indexed else IndexType.NONE,
            repeat_channels=indexed,
            repeat_schemas=indexed,
            use_chunking=indexed,
            use_statistics=indexed,
            use_summary_offsets=indexed,
        )
        writer.start()
        schema = writer.register_schema(
            "example.Timestamp", "jsonschema", b'{"type":"integer"}'
        )
        for topic, messages in samples.items():
            channel = writer.register_channel(topic, "json", schema)
            for time, value in messages:
                writer.add_message(
                    channel,
                    log_time=time * MILLISECOND,
                    data=str(value).encode(),
                    publish_time=0,
                )
        writer.finish()
    return path


@pytest.fixture
def source(tmp_path: Path) -> Path:
    return write_source(
        tmp_path / "source.mcap",
        {
            "/fast": [
                (1000, 1000),
                (1200, 1200),
                (1300, 1300),
                (1351, 1351),
                (1450, 1450),
                (1800, 1800),
            ],
            "/slow": [(1250, 1250), (1390, 1390)],
        },
    )


def invoke(source: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            sys.executable,
            "-m",
            "foxglove.mcap_alignment",
            source.name,
            OUTPUT_NAME,
            *args,
        ],
        cwd=source.parent,
        capture_output=True,
        text=True,
    )


def align(source: Path, *args: str) -> dict[str, list[tuple[int, int]]]:
    result = invoke(source, *args)
    assert result.returncode == 0, result.stderr
    samples: dict[str, list[tuple[int, int]]] = {}
    with (source.parent / OUTPUT_NAME).open("rb") as handle:
        for _, channel, message in make_reader(handle).iter_messages():
            target, remainder = divmod(message.log_time, MILLISECOND)
            assert remainder == 0
            samples.setdefault(channel.topic, []).append((target, int(message.data)))
    return samples


def test_fixed_rate_uses_previous_samples_through_the_recording_tail(
    source: Path,
) -> None:
    assert align(source, "--hz", "10") == {
        "/fast": [
            (1250, 1200),
            (1350, 1300),
            (1450, 1450),
            (1550, 1450),
            (1650, 1450),
            (1750, 1450),
        ],
        "/slow": [
            (1250, 1250),
            (1350, 1250),
            (1450, 1390),
            (1550, 1390),
            (1650, 1390),
            (1750, 1390),
        ],
    }


def test_reference_uses_its_timestamps_once_every_topic_has_started(
    source: Path,
) -> None:
    assert align(source, "--reference-topic", "/fast") == {
        "/fast": [(1300, 1300), (1351, 1351), (1450, 1450), (1800, 1800)],
        "/slow": [(1300, 1250), (1351, 1250), (1450, 1390), (1800, 1390)],
    }


def test_unindexed_input_is_aligned_by_log_time(tmp_path: Path) -> None:
    source = write_source(
        tmp_path / "source.mcap",
        {
            "/reference": [(1200, 12), (1000, 10), (1100, 11)],
            "/state": [(1150, 2), (1000, 1)],
        },
        indexed=False,
    )
    with source.open("rb") as handle:
        assert make_reader(handle).get_summary() is None
    assert align(source, "--hz", "10") == {
        "/reference": [(1000, 10), (1100, 11), (1200, 12)],
        "/state": [(1000, 1), (1100, 1), (1200, 2)],
    }


def test_indexed_input_can_include_top_level_messages(tmp_path: Path) -> None:
    source = tmp_path / "source.mcap"
    with source.open("wb") as handle:
        writer = Writer(handle, compression=CompressionType.NONE)
        writer.start()
        channel = writer.register_channel("/state", "json", schema_id=0)
        writer.add_message(channel, 1000 * MILLISECOND, b"10", 0)
        # The fixture needs one top-level message between two indexed chunks.
        writer._Writer__finalize_chunk()  # type: ignore[attr-defined]
        chunk_builder = writer._Writer__chunk_builder  # type: ignore[attr-defined]
        writer._Writer__chunk_builder = None  # type: ignore[attr-defined]
        writer.add_message(channel, 1100 * MILLISECOND, b"11", 0)
        writer._Writer__chunk_builder = chunk_builder  # type: ignore[attr-defined]
        writer.add_message(channel, 1200 * MILLISECOND, b"12", 0)
        writer.finish()

    with source.open("rb") as handle:
        summary = make_reader(handle).get_summary()
        assert summary is not None
        assert len(summary.chunk_indexes) == 2
    assert align(source, "--hz", "10") == {
        "/state": [(1000, 10), (1100, 11), (1200, 12)],
    }


def test_duplicate_reference_timestamp_produces_one_target(tmp_path: Path) -> None:
    source = write_source(
        tmp_path / "source.mcap",
        {"/reference": [(1000, 1), (1100, 2), (1100, 3)], "/state": [(1000, 4)]},
    )
    assert align(source, "--reference-topic", "/reference") == {
        "/reference": [(1000, 1), (1100, 3)],
        "/state": [(1000, 4), (1100, 4)],
    }


@pytest.mark.parametrize(
    ("timeline", "start", "end", "expected"),
    [
        (
            ("--hz", "10"),
            1375,
            1575,
            {
                "/fast": [(1375, 1351), (1475, 1450)],
                "/slow": [(1375, 1250), (1475, 1390)],
            },
        ),
        (
            ("--reference-topic", "/fast"),
            1325,
            1450,
            {"/fast": [(1351, 1351)], "/slow": [(1351, 1250)]},
        ),
    ],
)
def test_explicit_bounds_include_prior_samples_and_exclude_end_time(
    source: Path,
    timeline: tuple[str, str],
    start: int,
    end: int,
    expected: dict[str, list[tuple[int, int]]],
) -> None:
    assert (
        align(
            source,
            *timeline,
            "--start-ns",
            str(start * MILLISECOND),
            "--end-ns",
            str(end * MILLISECOND),
        )
        == expected
    )


def compressed_video_class() -> type[Message]:
    descriptor = descriptor_pb2.FileDescriptorProto(
        name="foxglove/CompressedVideo.proto",
        package="foxglove",
        syntax="proto3",
        dependency=["google/protobuf/timestamp.proto"],
    )
    video = descriptor.message_type.add(name="CompressedVideo")
    field = descriptor_pb2.FieldDescriptorProto
    video.field.add(
        name="timestamp",
        number=1,
        type=field.TYPE_MESSAGE,
        type_name=".google.protobuf.Timestamp",
    )
    video.field.add(name="frame_id", number=2, type=field.TYPE_STRING)
    video.field.add(name="data", number=3, type=field.TYPE_BYTES)
    video.field.add(name="format", number=4, type=field.TYPE_STRING)
    pool = descriptor_pool.DescriptorPool()
    pool.AddSerializedFile(TIMESTAMP_DESCRIPTOR.serialized_pb)
    pool.Add(descriptor)
    return message_factory.GetMessageClass(
        pool.FindMessageTypeByName("foxglove.CompressedVideo")
    )


def write_video_source(
    source: Path, *, corrupt_last_packet: bool = False
) -> Image.Image:
    key_image = Image.new("RGB", (16, 16), (30, 60, 90))
    delta_image = Image.new("RGB", (16, 16), (35, 60, 90))
    encoder = av.CodecContext.create("libx264rgb", "w")
    assert isinstance(encoder, av.video.codeccontext.VideoCodecContext)
    encoder.width, encoder.height = key_image.size
    encoder.pix_fmt = "rgb24"
    encoder.time_base = Fraction(1, 10)
    encoder.framerate = Fraction(10)
    encoder.max_b_frames = 0
    encoder.gop_size = 30
    encoder.options = {
        "crf": "0",
        "preset": "ultrafast",
        "tune": "zerolatency",
        "x264-params": "scenecut=0",
    }
    packets = []
    for index, image in enumerate((key_image, delta_image)):
        frame = av.VideoFrame.from_image(image)
        frame.pts = index
        packets.extend(encoder.encode(frame))
    packets.extend(encoder.encode(None))
    assert [packet.is_keyframe for packet in packets] == [True, False]

    compressed_video = compressed_video_class()
    with source.open("wb") as handle:
        writer = ProtobufWriter(handle, compression=CompressionType.NONE)
        for time, packet in zip((1000, 1100), packets):
            writer.write_message(
                "/camera",
                compressed_video(frame_id="camera", format="h264", data=bytes(packet)),
                log_time=time * MILLISECOND,
            )
        if corrupt_last_packet:
            writer.write_message(
                "/camera",
                compressed_video(frame_id="camera", format="h264", data=b"bad packet"),
                log_time=1200 * MILLISECOND,
            )
        for time in (1000, 1300):
            writer.write_message(
                "/state", Int32Value(value=time), log_time=time * MILLISECOND
            )
        writer.finish()
    return delta_image


@pytest.mark.skipif(not has_video_dependencies, reason="requires the alignment extra")
def test_video_uses_preroll_to_decode_and_reuse_delta_frame(tmp_path: Path) -> None:
    source = tmp_path / "source.mcap"
    delta_image = write_video_source(source)

    result = invoke(source, "--hz", "10", "--start-ns", str(1100 * MILLISECOND))
    assert result.returncode == 0, result.stderr
    camera_times = []
    with (tmp_path / OUTPUT_NAME).open("rb") as handle:
        reader = make_reader(handle, decoder_factories=[DecoderFactory()])
        for schema, _, message, decoded in reader.iter_decoded_messages(
            topics=["/camera"]
        ):
            assert schema is not None
            assert schema.name == "foxglove.CompressedImage"
            assert decoded.format == "png"
            assert decoded.frame_id == "camera"
            with Image.open(io.BytesIO(decoded.data)) as image:
                assert image.format == "PNG"
                assert image.size == delta_image.size
                assert image.convert("RGB").tobytes() == delta_image.tobytes()
            camera_times.append(message.log_time)
    assert camera_times == [time * MILLISECOND for time in (1100, 1200, 1300)]


@pytest.mark.skipif(not has_video_dependencies, reason="requires the alignment extra")
def test_late_decode_failure_does_not_publish_an_output(tmp_path: Path) -> None:
    source = tmp_path / "source.mcap"
    write_video_source(source, corrupt_last_packet=True)

    result = invoke(source, "--hz", "10")

    assert result.returncode != 0
    assert "/camera" in result.stderr
    assert set(tmp_path.iterdir()) == {source}


def test_existing_destination_is_preserved(source: Path) -> None:
    output = source.parent / OUTPUT_NAME
    output.write_bytes(b"existing destination")

    result = invoke(source, "--hz", "10")

    assert result.returncode != 0
    assert output.read_bytes() == b"existing destination"
    assert set(source.parent.iterdir()) == {source, output}


def test_sdk_align_returns_output_counts(source: Path) -> None:
    from foxglove.mcap_alignment import align as align_recording

    destination = source.parent / "api.mcap"
    report = align_recording(str(source), destination, hz=10)
    with destination.open("rb") as handle:
        messages = list(make_reader(handle).iter_messages())
    assert report == {"targets": 6, "topics": 2, "messages": len(messages)}
    assert len(messages) == 12
