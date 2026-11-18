import io
import json
import random
from pathlib import Path

import av
import pytest
from foxglove import Context, open_mcap
from foxglove._alignment_media import MediaError, process_image
from foxglove.channels import CompressedImageChannel
from foxglove.experimental.mcap_alignment import load_config, run
from foxglove.messages import CompressedImage
from mcap.reader import SeekingReader
from mcap.writer import CompressionType, Writer
from mcap_protobuf.decoder import DecoderFactory
from PIL import Image

EXAMPLE = Path(__file__).with_name("example")
ORIGIN = 1_700_000_000_000_000_000
PROVENANCE = "/foxglove/alignment/provenance/v1"

# Source frame ordinals expected at the five 10 Hz ticks, in camera order.
SELECTIONS = (
    (0, 0, 2, 2, 3),
    (0, 1, 2, 3, 5),
    (0, 0, 1, 2, 3),
    (0, 2, 3, 4, 5),
)

# Expected log times of the selected source frame ordinals, in milliseconds.
FRAME_TIMES = (
    {0: 0, 2: 199, 3: 301},
    {0: 0, 1: 77, 2: 158, 3: 242, 5: 399},
    {0: 0, 1: 127, 2: 249, 3: 381},
    {0: 0, 2: 99, 3: 151, 4: 299, 5: 351},
)


def _source_pixels(topic: str) -> list[bytes]:
    # Decode the complete fixture stream in file order. libswscale's RGB output
    # can vary by platform, so compare the selected frames on the same runner.
    with (EXAMPLE / "source.mcap").open("rb") as handle:
        reader = SeekingReader(handle, decoder_factories=[DecoderFactory()])
        packets = [
            video
            for _, _, _, video in reader.iter_decoded_messages(
                topics=[topic], log_time_order=False
            )
        ]

    codec = {"h264": "h264", "h265": "hevc"}[packets[0].format]
    decoder = av.CodecContext.create(codec, "r")
    frames = []
    for packet in packets:
        frames.extend(decoder.decode(av.Packet(packet.data)))
    frames.extend(decoder.decode(None))
    assert len(frames) == len(packets)
    return [frame.to_image().tobytes() for frame in frames]


def test_example_writes_indexed_images_and_provenance(tmp_path: Path) -> None:
    config = load_config(EXAMPLE / "job.json")
    destination = tmp_path / config["destination"]
    config.update(destination=str(destination), temporary_directory=str(tmp_path))
    report = run(config, base_directory=EXAMPLE)["report"]
    assert (report["targets"], report["selections"], report["missing_slots"]) == (
        5,
        20,
        0,
    )

    topics = [f"/aligned/camera/{camera}" for camera in range(4)]
    source_pixels = [_source_pixels(f"/camera/{camera}") for camera in range(4)]
    with destination.open("rb") as handle:
        reader = SeekingReader(
            handle, validate_crcs=True, decoder_factories=[DecoderFactory()]
        )
        summary = reader.get_summary()
        assert summary and summary.statistics
        assert summary.statistics.message_count == 40
        assert {channel.topic for channel in summary.channels.values()} == {
            *topics,
            PROVENANCE,
        }
        assert {
            channel_id
            for chunk in summary.chunk_indexes
            for channel_id in chunk.message_index_offsets
        } == set(summary.channels)

        for tick in range(5):
            target_time = ORIGIN + tick * 100_000_000
            images = list(
                reader.iter_decoded_messages(
                    topics=topics,
                    starting_at=target_time,
                    ending_before=target_time + 1,
                )
            )
            rows = list(
                reader.iter_messages(
                    topics=[PROVENANCE],
                    starting_at=target_time,
                    ending_before=target_time + 1,
                )
            )
            payloads = [json.loads(message.data) for _, _, message in rows]
            provenance = {row["output_topic"]: row for row in payloads}
            assert len(images) == len(rows) == 4
            assert {channel.topic for _, channel, _, _ in images} == set(topics)
            assert set(provenance) == set(topics)

            for schema, channel, message, image in images:
                camera = topics.index(channel.topic)
                frame = SELECTIONS[camera][tick]
                source_ms = FRAME_TIMES[camera][frame]
                source_time = ORIGIN + source_ms * 1_000_000

                assert schema and schema.name == "foxglove.CompressedImage"
                assert message.log_time == target_time
                assert message.sequence == tick
                assert message.publish_time == source_time + 17
                assert image.frame_id == f"camera-{camera}"
                assert image.timestamp.ToNanoseconds() == source_time - 23
                assert image.format == "png"
                with Image.open(io.BytesIO(image.data)) as png:
                    assert png.format == "PNG"
                    assert png.size == (64 + 8 * camera, 48 + 2 * camera)
                    assert png.mode == "RGB"
                    assert png.tobytes() == source_pixels[camera][frame]

                row = provenance[channel.topic]
                assert row["valid"] is True
                assert row["output_channel"] == channel.id
                assert row["output_sequence"] == row["target_index"] == tick
                assert row["target_time"] == str(target_time)
                assert row["source_file"] == 0
                assert row["source_channel"] == camera + 1
                assert row["source_sequence"] == camera * 100 + frame
                assert row["source_log_time"] == str(source_time)
                assert row["source_publish_time"] == str(message.publish_time)
                assert row["offset_ns"] == str(source_time - target_time)
                assert row["reused"] == (
                    tick > 0 and SELECTIONS[camera][tick - 1] == frame
                )


def test_reference_camera_aligns_video_and_telemetry(tmp_path: Path) -> None:
    source = tmp_path / "reference.mcap"
    destination = tmp_path / "aligned.mcap"
    camera = "/camera/1"
    image_topic = "/aligned/camera"
    telemetry_topic = "/aligned/telemetry"

    def at_ms(milliseconds: int) -> int:
        return ORIGIN + milliseconds * 1_000_000

    telemetry = [(70, b'{"position":1}'), (145, b'{"position":2}')]
    with (
        (EXAMPLE / "source.mcap").open("rb") as input_file,
        source.open("wb") as output,
    ):
        writer = Writer(output, compression=CompressionType.NONE)
        writer.start()
        reader = SeekingReader(input_file, decoder_factories=[DecoderFactory()])
        summary = reader.get_summary()
        assert summary is not None
        channel = next(c for c in summary.channels.values() if c.topic == camera)
        schema = summary.schemas[channel.schema_id]
        schema_id = writer.register_schema(schema.name, schema.encoding, schema.data)
        camera_id = writer.register_channel(camera, channel.message_encoding, schema_id)
        telemetry_id = writer.register_channel("/telemetry", "json", 0)
        for _, _, message, video in reader.iter_decoded_messages(
            topics=[camera], log_time_order=False
        ):
            data = message.data
            if message.log_time == at_ms(321):
                video.data = b"invalid Annex B packet"
                data = video.SerializeToString()
            writer.add_message(
                camera_id,
                message.log_time,
                data,
                message.publish_time,
                sequence=message.sequence,
            )
        for milliseconds, payload in telemetry:
            writer.add_message(
                telemetry_id, at_ms(milliseconds), payload, at_ms(milliseconds) + 11
            )
        writer.finish()

    config = load_config(EXAMPLE / "job.json")
    config.update(
        paths=[str(source)],
        destination=str(destination),
        episode={"start": at_ms(70), "end": at_ms(243)},
        timeline={"kind": "reference", "topic": camera},
        streams=[
            {
                **config["streams"][1],
                "output_topic": image_topic,
                "policy": "exact",
                "tolerance_ns": 0,
            },
            {
                "topic": "/telemetry",
                "output_topic": telemetry_topic,
                "policy": "previous",
                "tolerance_ns": 100_000_000,
                "required": True,
                "independence": "declared",
            },
        ],
    )
    report = run(config, base_directory=tmp_path)["report"]
    assert (report["targets"], report["selections"], report["reused"]) == (3, 6, 1)
    assert report["media_video_packets"] == report["media_decoded_frames"] == 4
    assert report["media_dependency_frames"] == 1
    source_pixels = _source_pixels(camera)

    with destination.open("rb") as handle:
        reader = SeekingReader(
            handle, validate_crcs=True, decoder_factories=[DecoderFactory()]
        )
        for tick, (frame, telemetry_index) in enumerate(((1, 0), (2, 1), (3, 1))):
            target = at_ms(FRAME_TIMES[1][frame])
            [(schema, _, image_message, image)] = list(
                reader.iter_decoded_messages(
                    topics=[image_topic], starting_at=target, ending_before=target + 1
                )
            )
            assert schema and schema.name == "foxglove.CompressedImage"
            assert image_message.log_time == target
            assert image_message.sequence == tick
            with Image.open(io.BytesIO(image.data)) as png:
                assert png.mode == "RGB"
                assert png.tobytes() == source_pixels[frame]

            [(_, _, message)] = list(
                reader.iter_messages(
                    topics=[telemetry_topic],
                    starting_at=target,
                    ending_before=target + 1,
                )
            )
            source_ms, payload = telemetry[telemetry_index]
            assert (message.log_time, message.publish_time, message.data) == (
                target,
                at_ms(source_ms) + 11,
                payload,
            )
            rows = reader.iter_messages(
                topics=[PROVENANCE], starting_at=target, ending_before=target + 1
            )
            payloads = [json.loads(record.data) for _, _, record in rows]
            provenance = {row["output_topic"]: row for row in payloads}
            assert set(provenance) == {image_topic, telemetry_topic}
            assert provenance[image_topic]["source_log_time"] == str(target)
            assert provenance[telemetry_topic]["source_log_time"] == str(
                at_ms(source_ms)
            )
            assert provenance[telemetry_topic]["reused"] is (tick == 2)


def test_transcoded_image_can_exceed_input_record_limit(tmp_path: Path) -> None:
    input_limit = 8 * 1024 * 1024
    size = (2560, 1440)
    rgb = random.Random(1588).randbytes(size[0] * size[1] * 3)
    jpeg = io.BytesIO()
    Image.frombytes("RGB", size, rgb).save(jpeg, "JPEG", quality=95)
    assert len(jpeg.getbuffer()) < input_limit

    source = tmp_path / "image.mcap"
    destination = tmp_path / "aligned.mcap"
    context = Context()
    with open_mcap(source, context=context):
        channel = CompressedImageChannel("/image", context=context)
        channel.log(
            CompressedImage(data=jpeg.getvalue(), format="jpeg"), log_time=ORIGIN
        )

    run(
        {
            "paths": [str(source)],
            "destination": str(destination),
            "episode": {"start": ORIGIN, "end": ORIGIN + 1},
            "timeline": {"kind": "reference", "topic": "/image"},
            "streams": [
                {
                    "topic": "/image",
                    "output_topic": "/aligned/image",
                    "policy": "exact",
                    "tolerance_ns": 0,
                    "required": True,
                    "media": {"calibration": {"status": "unavailable"}},
                }
            ],
            "limits": {
                "max_record_bytes": input_limit,
                "max_chunk_bytes": input_limit,
                "max_decompressed_chunk_bytes": input_limit,
            },
        },
        base_directory=tmp_path,
    )

    with destination.open("rb") as handle:
        reader = SeekingReader(
            handle, validate_crcs=True, decoder_factories=[DecoderFactory()]
        )
        [(_, _, message, image)] = list(
            reader.iter_decoded_messages(topics=["/aligned/image"], starting_at=ORIGIN)
        )
        assert message.log_time == ORIGIN
        assert len(image.data) > input_limit
        with Image.open(io.BytesIO(image.data)) as png:
            assert png.format == "PNG"
            assert png.size == size
            png.load()


@pytest.mark.parametrize("expected_format", ["jpeg", "png"])
def test_image_does_not_open_an_undeclared_format(
    expected_format: str, monkeypatch: pytest.MonkeyPatch
) -> None:
    gif = io.BytesIO()
    Image.new("RGB", (8, 8)).save(gif, "GIF")

    def reject_open(*args: object, **kwargs: object) -> None:
        raise AssertionError("GIF decoder was called")

    _, accept_gif = Image.OPEN["GIF"]
    monkeypatch.setitem(Image.OPEN, "GIF", (reject_open, accept_gif))
    with pytest.raises(MediaError) as error:
        process_image(gif.getvalue(), expected_format)
    assert error.value.kind == "decode_failed"
