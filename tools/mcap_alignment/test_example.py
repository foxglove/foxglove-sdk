import hashlib
import io
import json
import random
from pathlib import Path

from foxglove import Context, open_mcap
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

# Retained synthetic source frames: (log time in ms from ORIGIN, frozen RGB SHA-256).
FRAMES = (
    {
        0: (0, "ce8d862154798a65c6c236fc882a89fdef35fa3633555f1a2a45924352bf0eb0"),
        2: (199, "311609d6ad6dd83c953ba0b915d5baf22c98889dd81e4bac7f830e2e54bf78cc"),
        3: (301, "a1a947a0d5647dc195c1d3ad8759a21213f58cc192c7213fe9171cb4e5d87f21"),
    },
    {
        0: (0, "427d0cd02c0cd475357a315793e46de8be9fe3bff9785f7cd47be987b927f6e5"),
        1: (77, "b7c7bd82e8aec6edba986c9826104eae09713cc188dffb987f3bcb64a005d855"),
        2: (158, "3eb7e4a9a9b85cf22c056664128158b78e75f90ba94a55103f7fa6a9f6291cae"),
        3: (242, "24906e707cdaf26b466179b14f08c18e87f1b8d546698dbdedf41baca0d73eb0"),
        5: (399, "5325e4efa7fe8bda8a943d595c8d8f8703c986fbff1df509f3a30ededb6116b7"),
    },
    {
        0: (0, "27ea12eb1c544e96012e33825a2e5a4d061fbb90c60e4ea4c6c0d850d885374e"),
        1: (127, "c97a9170ba67484be3db82b9ebe7174b467b572326950a021433264b2add87d6"),
        2: (249, "aeb42ded1be170e1ff0a8cb4979a1084870647896ae4f2cb7ba21fdac562c04f"),
        3: (381, "9d70f14ba27da11503d2410c0a8871a776cf37a83e451b085367bd6affee252f"),
    },
    {
        0: (0, "8d62c06df8236a9a49d5a1d671fe15ca0fd07ac5a8a21a88a96aa2c383743248"),
        2: (99, "fa558ceed6f55181c1eebdf19b539e38245a9303eb83503fe2d56402b2198d5d"),
        3: (151, "335c5577426bd324a77f746f75a2ab19ce5f8740739904370a4e9840d792f2cd"),
        4: (299, "27d9a2df23a28d840f30f7e549a5d153d8732e9dbd4c1c56fa5bf4c5b63131cc"),
        5: (351, "24107bd81da14c94216f17edb1ea05c132c29d060c71602477ce99334cb61018"),
    },
)


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
                source_ms, digest = FRAMES[camera][frame]
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
                    assert (
                        hashlib.sha256(png.convert("RGB").tobytes()).hexdigest()
                        == digest
                    )

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
        reader = SeekingReader(input_file)
        summary = reader.get_summary()
        assert summary is not None
        channel = next(c for c in summary.channels.values() if c.topic == camera)
        schema = summary.schemas[channel.schema_id]
        schema_id = writer.register_schema(schema.name, schema.encoding, schema.data)
        camera_id = writer.register_channel(camera, channel.message_encoding, schema_id)
        telemetry_id = writer.register_channel("/telemetry", "json", 0)
        for _, _, message in reader.iter_messages(
            topics=[camera], log_time_order=False
        ):
            writer.add_message(
                camera_id,
                message.log_time,
                message.data,
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

    with destination.open("rb") as handle:
        reader = SeekingReader(
            handle, validate_crcs=True, decoder_factories=[DecoderFactory()]
        )
        for tick, (frame, telemetry_index) in enumerate(((1, 0), (2, 1), (3, 1))):
            milliseconds, digest = FRAMES[1][frame]
            target = at_ms(milliseconds)
            [(schema, _, image_message, image)] = list(
                reader.iter_decoded_messages(
                    topics=[image_topic], starting_at=target, ending_before=target + 1
                )
            )
            assert schema and schema.name == "foxglove.CompressedImage"
            assert image_message.log_time == target
            assert image_message.sequence == tick
            with Image.open(io.BytesIO(image.data)) as png:
                assert (
                    hashlib.sha256(png.convert("RGB").tobytes()).hexdigest() == digest
                )

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
