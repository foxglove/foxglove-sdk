"""Write one latest-at-or-before sample per topic at each output log time.

    foxglove-align-mcap input.mcap aligned.mcap --hz 10
    foxglove-align-mcap input.mcap aligned.mcap --reference-topic /camera/front

The default output starts once every nonempty topic has a sample and ends at
the last input log time. Original publish times and embedded timestamps are
preserved. Video becomes independently readable PNG frames, which can make
the output larger. Unindexed files with disordered timestamps are sorted in
memory. The output contains sampled messages and their channel definitions.
"""

from __future__ import annotations

import argparse
import json
import os
import struct
import tempfile
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from fractions import Fraction
from itertools import groupby
from operator import itemgetter
from pathlib import Path
from typing import BinaryIO, Iterator

from foxglove._mcap_alignment_video import VideoDecoder, is_video, register_image_schema

try:
    from mcap.data_stream import ReadDataStream
    from mcap.exceptions import McapError
    from mcap.opcode import Opcode
    from mcap.reader import McapReader, NonSeekingReader, make_reader
    from mcap.records import Channel, Message, MessageIndex, Schema
    from mcap.stream_reader import MAGIC_SIZE
    from mcap.summary import Summary
    from mcap.writer import CompressionType, Writer
except ModuleNotFoundError as error:
    if error.name != "mcap":
        raise
    raise ImportError("MCAP alignment requires 'foxglove-sdk[alignment]'") from error


__all__ = ["align"]

NANOS_PER_SECOND = 1_000_000_000
MAX_TIMESTAMP = (1 << 64) - 1
MessageRecord = tuple[Schema | None, Channel, Message]
RECORD_HEADER = struct.Struct("<BQ")


def timestamp(value: str) -> int:
    result = int(value)
    if not 0 <= result <= MAX_TIMESTAMP:
        raise argparse.ArgumentTypeError("expected an unsigned 64-bit timestamp")
    return result


def rate(value: str) -> Fraction:
    try:
        result = Fraction(value)
    except (ValueError, ZeroDivisionError) as error:
        raise argparse.ArgumentTypeError(
            "expected a rate such as 10 or 30000/1001"
        ) from error
    if not 0 < result <= NANOS_PER_SECOND:
        raise argparse.ArgumentTypeError("rate must be positive and at most 1 GHz")
    return result


def check_video_time(topic: str, previous: int, current: int) -> None:
    if current < previous:
        raise ValueError(
            f"{topic}: video log times must be nondecreasing in file order"
        )


def complete_chunk_index(source: BinaryIO, summary: Summary) -> bool:
    """Check for top-level messages and verify video order without reading payloads."""
    chunks = {chunk.chunk_start_offset: chunk for chunk in summary.chunk_indexes}
    video_times = {
        channel.id: 0
        for channel in summary.channels.values()
        if is_video(summary.schemas.get(channel.schema_id), channel)
    }
    offset = MAGIC_SIZE
    while True:
        source.seek(offset)
        header = source.read(RECORD_HEADER.size)
        if len(header) != RECORD_HEADER.size:
            raise McapError("unexpected end of MCAP before DataEnd")
        opcode, length = RECORD_HEADER.unpack(header)
        record_end = offset + RECORD_HEADER.size + length
        if opcode == Opcode.MESSAGE:
            return False
        if opcode == Opcode.DATA_END:
            return not chunks
        if opcode != Opcode.CHUNK:
            offset = record_end
            continue

        chunk = chunks.pop(offset, None)
        if chunk is None or chunk.chunk_length != record_end - offset:
            return False
        offset = record_end + chunk.message_index_length
        if video_times and not chunk.message_index_offsets:
            return False
        for channel_id in video_times.keys() & chunk.message_index_offsets.keys():
            index_offset = chunk.message_index_offsets[channel_id]
            if not record_end <= index_offset < offset:
                return False
            source.seek(index_offset)
            index_header = source.read(RECORD_HEADER.size)
            if len(index_header) != RECORD_HEADER.size:
                return False
            index_opcode, index_length = RECORD_HEADER.unpack(index_header)
            if (
                index_opcode != Opcode.MESSAGE_INDEX
                or index_offset + RECORD_HEADER.size + index_length > offset
            ):
                return False
            index = MessageIndex.read(ReadDataStream(source))
            if index.channel_id != channel_id:
                return False
            previous = video_times[channel_id]
            topic = summary.channels[channel_id].topic
            for time, _ in sorted(index.records, key=itemgetter(1)):
                check_video_time(topic, previous, time)
                previous = time
            video_times[channel_id] = previous


def read_source(
    source: BinaryIO, reader: McapReader
) -> tuple[set[str], Iterator[MessageRecord]]:
    summary = reader.get_summary()
    if summary is not None and summary.chunk_indexes and summary.statistics is not None:
        stats = summary.statistics
        counts = stats.channel_message_counts
        if (
            stats.chunk_count == len(summary.chunk_indexes)
            and sum(counts.values()) == stats.message_count
            and counts.keys() <= summary.channels.keys()
            and all(
                channel.schema_id == 0 or channel.schema_id in summary.schemas
                for channel in summary.channels.values()
            )
            and complete_chunk_index(source, summary)
        ):
            topics = {
                summary.channels[channel_id].topic
                for channel_id, count in counts.items()
                if count
            }
            return topics, reader.iter_messages()

    # Without a complete index, determine topic membership and physical ordering
    # in a linear pass. Only out-of-order unindexed input needs an in-memory sort.
    source.seek(0)
    topics = set()
    in_order = True
    last_time = 0
    video_times: dict[int, int | None] = {}
    for schema, channel, message in NonSeekingReader(source).iter_messages(
        log_time_order=False
    ):
        if channel.id not in video_times:
            topics.add(channel.topic)
            video_times[channel.id] = 0 if is_video(schema, channel) else None
        previous = video_times[channel.id]
        if previous is not None:
            check_video_time(channel.topic, previous, message.log_time)
            video_times[channel.id] = message.log_time
        in_order &= message.log_time >= last_time
        last_time = message.log_time
    source.seek(0)
    return topics, NonSeekingReader(source).iter_messages(log_time_order=not in_order)


@dataclass(slots=True)
class SourceChannel:
    schema: Schema | None
    channel: Channel
    decoder: VideoDecoder | None
    output_id: int | None = None


@dataclass(slots=True)
class Sample:
    source: SourceChannel
    message: Message
    image: bytes | None = None

    def discard(self) -> None:
        if self.source.decoder is not None and self.image is None:
            self.source.decoder.feed(self.message.data, selected=False)

    def payload(self) -> bytes:
        if self.source.decoder is None:
            return self.message.data
        if self.image is None:
            self.image = self.source.decoder.feed(self.message.data, selected=True)
        if self.image is None:
            raise ValueError(
                f"{self.source.channel.topic}: video decoder returned no image"
            )
        return self.image


def write_aligned(
    writer: Writer,
    source: BinaryIO,
    *,
    workers: ThreadPoolExecutor,
    hz: Fraction | None,
    reference_topic: str | None,
    start_ns: int,
    end_ns: int | None,
) -> dict[str, int]:
    reader = make_reader(source)
    header = reader.get_header()
    topics, records = read_source(source, reader)
    if not topics:
        raise ValueError("source contains no messages")
    if reference_topic is not None and reference_topic not in topics:
        raise ValueError(f"reference topic {reference_topic!r} contains no messages")

    writer.start(profile=header.profile)
    channels: dict[int, SourceChannel] = {}
    latest: dict[str, Sample] = {}
    schemas: dict[int, int] = {}
    image_schema_id = None
    video_channels = 0
    targets = 0

    def emit(target: int) -> None:
        nonlocal image_schema_id, targets
        if video_channels > 1:
            pending = [
                sample
                for sample in latest.values()
                if sample.source.decoder is not None and sample.image is None
            ]
            if len(pending) > 1:
                # Each camera owns its decoder. Join before advancing any channel.
                list(workers.map(Sample.payload, pending))
        for sample in latest.values():
            stream = sample.source
            if stream.output_id is None:
                message_encoding = stream.channel.message_encoding
                schema_id = 0
                if stream.decoder is not None:
                    if image_schema_id is None:
                        image_schema_id = register_image_schema(writer)
                    schema_id = image_schema_id
                    message_encoding = "protobuf"
                elif stream.schema is not None:
                    schema = stream.schema
                    if schema.id not in schemas:
                        schemas[schema.id] = writer.register_schema(
                            schema.name, schema.encoding, schema.data
                        )
                    schema_id = schemas[schema.id]
                stream.output_id = writer.register_channel(
                    stream.channel.topic,
                    message_encoding,
                    schema_id,
                    stream.channel.metadata,
                )
            writer.add_message(
                stream.output_id,
                target,
                sample.payload(),
                sample.message.publish_time,
                sequence=sample.message.sequence,
            )
        targets += 1

    period = NANOS_PER_SECOND * hz.denominator if hz is not None else 0
    divisor = hz.numerator if hz is not None else 1
    origin: int | None = None
    next_target: int | None = None
    last_time = None

    def emit_grid_before(limit: int) -> None:
        nonlocal next_target
        if next_target is None or origin is None:
            return
        while next_target < limit:
            emit(next_target)
            next_target = origin + (targets * period + divisor - 1) // divisor

    for time, group in groupby(records, key=lambda record: record[2].log_time):
        if end_ns is not None and time >= end_ns:
            break
        emit_grid_before(time)
        reference_present = False
        for schema, channel, message in group:
            topic = channel.topic
            if topic in latest:
                latest[topic].discard()
            if channel.id not in channels:
                decoder = None
                if is_video(schema, channel):
                    if schema is None:
                        raise ValueError(f"{topic}: video channel is missing a schema")
                    decoder = VideoDecoder(schema, channel)
                    video_channels += 1
                channels[channel.id] = SourceChannel(schema, channel, decoder)
            latest[topic] = Sample(channels[channel.id], message)
            reference_present |= topic == reference_topic
        last_time = time
        if len(latest) != len(topics):
            continue
        if hz is not None and origin is None:
            origin = next_target = max(time, start_ns)
        elif reference_present and time >= start_ns:
            emit(time)

    if last_time is not None:
        emit_grid_before(end_ns if end_ns is not None else last_time + 1)
    if targets == 0:
        raise ValueError("no complete target in the requested time range")
    for stream in channels.values():
        if stream.decoder is not None:
            stream.decoder.finish()
    writer.finish()
    return {
        "targets": targets,
        "topics": len(topics),
        "messages": targets * len(topics),
    }


def align(
    source: str | Path,
    destination: str | Path,
    *,
    hz: int | float | Fraction | None = None,
    reference_topic: str | None = None,
    start_ns: int = 0,
    end_ns: int | None = None,
) -> dict[str, int]:
    """Align a local MCAP by log time and write to a new destination.

    Supply exactly one of a positive frequency in Hz or a reference topic.
    Output begins once every nonempty topic has a value. The optional target
    bounds are inclusive at the start and exclusive at the end, in nanoseconds.
    Returns counts of output targets, populated topics, and written messages.
    """
    if (hz is None) == (reference_topic is None):
        raise ValueError("choose exactly one of hz and reference_topic")
    frequency = None if hz is None else Fraction(str(hz))
    if frequency is not None and not 0 < frequency <= NANOS_PER_SECOND:
        raise ValueError("hz must be positive and at most 1 GHz")
    if type(start_ns) is not int or not 0 <= start_ns <= MAX_TIMESTAMP:
        raise ValueError("start_ns must be an unsigned 64-bit timestamp")
    if end_ns is not None and (
        type(end_ns) is not int or not start_ns < end_ns <= MAX_TIMESTAMP
    ):
        raise ValueError("end_ns must be an unsigned 64-bit timestamp after start_ns")
    output_path = Path(destination)
    if os.path.lexists(output_path):
        raise FileExistsError(f"output already exists: {destination}")
    staged_path = None
    try:
        with (
            ThreadPoolExecutor(max_workers=4) as workers,
            Path(source).open("rb") as input_file,
            tempfile.NamedTemporaryFile(
                dir=output_path.parent, prefix=".align-mcap-", delete=False
            ) as staged,
        ):
            staged_path = Path(staged.name)
            report = write_aligned(
                Writer(staged, compression=CompressionType.LZ4),
                input_file,
                workers=workers,
                hz=frequency,
                reference_topic=reference_topic,
                start_ns=start_ns,
                end_ns=end_ns,
            )
        os.link(staged_path, output_path)
        return report
    finally:
        if staged_path is not None:
            staged_path.unlink()


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Write one latest-at-or-before sample per topic at each output log time.",
        epilog=(
            "Run with foxglove-align-mcap. Targets start after every "
            "populated topic has a sample. Equal-time input messages use the last "
            "message in file order. Earlier messages remain eligible with --start-ns. "
            "Non-video payloads are assumed to be independent. Supported Protobuf "
            "Foxglove H.264/H.265, VP9, and AV1 video becomes PNG at the original topic "
            "and requires one immediately decoded frame per packet in per-channel "
            "log-time order."
        ),
    )
    parser.add_argument("input", type=Path, help="source MCAP")
    parser.add_argument("output", type=Path, help="new aligned MCAP")
    timeline = parser.add_mutually_exclusive_group(required=True)
    timeline.add_argument(
        "--hz", type=rate, help="fixed rate in Hz, including fractional rates"
    )
    timeline.add_argument("--reference-topic", help="topic supplying target log times")
    parser.add_argument(
        "--start-ns", type=timestamp, default=0, help="inclusive lower target bound"
    )
    parser.add_argument("--end-ns", type=timestamp, help="exclusive upper target bound")
    args = parser.parse_args()
    if args.end_ns is not None and args.start_ns >= args.end_ns:
        parser.error("--start-ns must be less than --end-ns")
    try:
        report = align(
            args.input,
            args.output,
            hz=args.hz,
            reference_topic=args.reference_topic,
            start_ns=args.start_ns,
            end_ns=args.end_ns,
        )
    except (McapError, OSError, ValueError) as error:
        parser.exit(1, f"align_mcap: {error}\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
