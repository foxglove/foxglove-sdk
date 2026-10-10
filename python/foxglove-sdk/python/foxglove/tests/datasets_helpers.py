"""Shared dataset test clients, storage handles, and MCAP fixtures."""

import io
import json
from collections.abc import Generator, Iterator
from datetime import datetime, timezone
from typing import IO, Any
from unittest.mock import MagicMock

from foxglove.datasets.storage import ObjectLocation
from mcap.decoder import DecoderFactory
from mcap.records import Channel, Message, Schema
from mcap.writer import CompressionType, IndexType, Writer

START = datetime(2026, 1, 1, tzinfo=timezone.utc)
START_NS = 1767225600 * 1_000_000_000


class Page:
    def auto_paging_iter(self) -> Iterator[dict[str, Any]]:
        # Deliberately non-sorted, spanning more than one yielded batch.
        for ids in (("c", "a"), ("d", "b")):
            for episode_id in ids:
                yield {
                    "has_missing_recordings": False,
                    "episode": {
                        "id": episode_id,
                        "start_time": datetime(2026, 1, 1, tzinfo=timezone.utc),
                        "end_time": datetime(2026, 1, 2, tzinfo=timezone.utc),
                        "metadata": {"label": episode_id},
                    },
                }


class Client:
    def __init__(self) -> None:
        self.calls: list[tuple[str, list[str]]] = []
        self.closed: list[str] = []
        self.time_ranges: list[tuple[datetime | None, datetime | None]] = []

    def get_dataset_version(
        self, *, dataset_id: str, version_number: int
    ) -> dict[str, Any]:
        assert (dataset_id, version_number) == ("dataset", 7)
        return {"committed_at": "2026-01-01", "has_missing_recordings": False}

    def get_dataset_version_episodes(
        self,
        *,
        dataset_id: str,
        version_number: int,
        limit: int,
        include_recordings: bool = False,
    ) -> Page:
        assert (dataset_id, version_number, limit) == ("dataset", 7, 2000)
        return Page()

    def iter_messages(
        self,
        *,
        episode_id: str,
        topics: list[str],
        decoder_factories: list[DecoderFactory] | None = None,
        start: datetime | None = None,
        end: datetime | None = None,
    ) -> Generator[tuple[Schema | None, Channel, Message, Any], None, None]:
        assert decoder_factories is None
        self.calls.append((episode_id, topics))
        self.time_ranges.append((start, end))
        try:
            yield (
                Schema(id=1, data=b"{}", encoding="jsonschema", name="Episode"),
                Channel(
                    id=1,
                    schema_id=1,
                    topic=topics[0],
                    message_encoding="json",
                    metadata={},
                ),
                Message(
                    channel_id=1,
                    log_time=0,
                    publish_time=0,
                    sequence=0,
                    data=json.dumps(episode_id).encode(),
                ),
                episode_id,
            )
            raise AssertionError("Read past the requested sample")
        finally:
            self.closed.append(episode_id)


class CountingFile(io.BytesIO):
    def __init__(self, data: bytes, *, seekable: bool = True) -> None:
        super().__init__(data)
        self.ranges: list[tuple[int, int]] = []
        self._seekable = seekable

    def read(self, size: int | None = -1) -> bytes:
        offset = self.tell()
        data = super().read(size)
        self.ranges.append((offset, len(data)))
        return data

    def readinto(self, buffer: Any) -> int:
        data = self.read(len(buffer))
        buffer[: len(data)] = data
        return len(data)

    def seekable(self) -> bool:
        return self._seekable

    @property
    def read_bytes(self) -> int:
        return sum(size for _, size in self.ranges)


class Store:
    def __init__(
        self,
        objects: dict[ObjectLocation, bytes],
        *,
        seekable: bool = True,
    ) -> None:
        self.objects = objects
        self.files: list[CountingFile] = []
        self.locations: list[ObjectLocation] = []
        self.seekable = seekable

    def open(self, location: ObjectLocation) -> IO[bytes]:
        self.locations.append(location)
        file = CountingFile(self.objects[location], seekable=self.seekable)
        self.files.append(file)
        return file


def mcap_bytes(
    messages: list[tuple[str, int, Any]],
    *,
    schema_name: str = "Measurement",
    encoding: str = "json",
    indexed: bool = True,
    message_indexes: bool = True,
) -> bytes:
    output = io.BytesIO()
    writer = Writer(
        output,
        chunk_size=1,
        compression=CompressionType.NONE,
        use_chunking=indexed,
        index_types=IndexType.ALL if message_indexes else IndexType.CHUNK,
    )
    writer.start()
    schema = writer.register_schema(schema_name, "jsonschema", b"{}")
    channels = {
        topic: writer.register_channel(topic, encoding, schema)
        for topic in dict.fromkeys(topic for topic, _, _ in messages)
    }
    for topic, offset_ns, payload in messages:
        stamp = START_NS + offset_ns
        writer.add_message(channels[topic], stamp, json.dumps(payload).encode(), stamp)
    writer.finish()
    return output.getvalue()


def plan_client(recordings: Any) -> MagicMock:
    client = MagicMock()
    client.get_dataset_version.return_value = {"committed_at": START}
    client.get_dataset_version_episodes.return_value.auto_paging_iter.return_value = (
        iter(
            [
                {
                    "episode": {
                        "id": "episode",
                        "start_time": START,
                        "end_time": START,
                        "metadata": {},
                        "recordings": recordings,
                    }
                }
            ]
        )
    )
    return client


def mcap_with_unindexed_chunk(
    *, message_time: int | None, statistics: bool, compressed: bool
) -> bytes:
    """Write an indexed message followed by a chunk without message indexes."""
    from mcap.data_stream import RecordBuilder
    from mcap.records import (
        Channel,
        Chunk,
        ChunkIndex,
        DataEnd,
        Footer,
        Header,
        Message,
        MessageIndex,
        Statistics,
    )

    def pack(*records: Any) -> bytes:
        builder = RecordBuilder()
        for record in records:
            record.write(builder)
        return bytes(builder.end())

    magic = b"\x89MCAP0\r\n"
    output = io.BytesIO()
    output.write(magic + pack(Header(profile="", library="test")))
    channel = Channel(
        id=1, topic="/selected", message_encoding="json", metadata={}, schema_id=0
    )
    unused = Channel(
        id=2, topic="/unused", message_encoding="json", metadata={}, schema_id=0
    )
    indexes = []
    for indexed in (True, False):
        stamp = START_NS if indexed else (message_time or 0)
        records: list[Channel | Message] = [channel] if indexed else [unused]
        if indexed or message_time is not None:
            records.append(
                Message(
                    channel_id=1,
                    log_time=stamp,
                    data=b"42",
                    publish_time=stamp,
                    sequence=0,
                )
            )
        raw = pack(*records)
        if compressed:
            import zstandard

            data = zstandard.compress(raw)
        else:
            data = raw
        compression = "zstd" if compressed else ""
        offset = output.tell()
        chunk = pack(
            Chunk(
                compression=compression,
                data=data,
                message_end_time=stamp,
                message_start_time=stamp,
                uncompressed_crc=0,
                uncompressed_size=len(raw),
            )
        )
        output.write(chunk)
        message_offset = output.tell()
        message_index = (
            pack(MessageIndex(channel_id=1, records=[(stamp, len(pack(channel)))]))
            if indexed
            else b""
        )
        output.write(message_index)
        indexes.append(
            ChunkIndex(
                chunk_length=len(chunk),
                chunk_start_offset=offset,
                compression=compression,
                compressed_size=len(data),
                message_end_time=stamp,
                message_index_length=len(message_index),
                message_index_offsets={1: message_offset} if indexed else {},
                message_start_time=stamp,
                uncompressed_size=len(raw),
            )
        )
    output.write(pack(DataEnd(data_section_crc=0)))
    summary_start = output.tell()
    output.write(pack(channel, unused))
    if statistics:
        count = 1 if message_time is None else 2
        output.write(
            pack(
                Statistics(
                    attachment_count=0,
                    channel_count=2,
                    channel_message_counts={1: count},
                    chunk_count=2,
                    message_count=count,
                    message_end_time=START_NS,
                    message_start_time=(
                        min(START_NS, message_time)
                        if message_time is not None
                        else START_NS
                    ),
                    metadata_count=0,
                    schema_count=0,
                )
            )
        )
    output.write(
        pack(
            *indexes,
            Footer(summary_start=summary_start, summary_offset_start=0, summary_crc=0),
        )
        + magic
    )
    return output.getvalue()
