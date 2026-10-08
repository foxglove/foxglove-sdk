"""Direct, indexed MCAP reads from customer-managed object storage."""

from __future__ import annotations

import copy
import heapq
from collections.abc import Callable, Generator, Sequence
from contextlib import ExitStack
from dataclasses import dataclass, replace
from datetime import datetime, timezone
from io import BufferedReader, RawIOBase
from typing import IO, TYPE_CHECKING, Any, Protocol, cast

if TYPE_CHECKING:
    from mcap.decoder import DecoderFactory
    from mcap.records import Channel, Message, Schema


@dataclass(frozen=True)
class ObjectLocation:
    """Location of an original recording in customer-managed storage.

    ``path`` is the full object key, not the recording's display filename.
    For Azure, ``bucket`` is the container and ``azure_storage_account_name``
    identifies its account. These APIs are experimental and unstable.
    """

    bucket: str
    path: str
    azure_storage_account_name: str | None = None


class ObjectStore(Protocol):
    """Worker-local storage access using the customer's credentials.

    Implementations may use PyArrow, fsspec, or another random-access filesystem.
    These APIs are experimental and unstable.
    """

    def open(self, location: ObjectLocation) -> IO[bytes]:
        """Open a fresh seekable binary file; the SDK owns and closes this handle.

        The handle must support ``readinto`` as well as ``read`` and ``seek``.
        The SDK buffers small reads in 64 KiB blocks. Use range reads and bounded
        caching, not a whole-object download.
        Files must remain immutable for the duration of a dataset read.
        """
        ...


ObjectStoreFactory = Callable[[], ObjectStore]


class _MessageIds:
    """Keep schema/channel IDs consistent across all streams of an episode."""

    def __init__(self) -> None:
        self.schemas: dict[tuple[str, str, bytes], Schema] = {}
        self.channels: dict[tuple[Any, ...], Channel] = {}

    def remap(
        self, schema: Schema | None, channel: Channel
    ) -> tuple[Schema | None, Channel]:
        output_schema = None
        if schema is not None:
            schema_key = (schema.name, schema.encoding, schema.data)
            output_schema = self.schemas.get(schema_key)
            if output_schema is None:
                output_schema = replace(schema, id=len(self.schemas) + 1)
                self.schemas[schema_key] = output_schema
        schema_id = output_schema.id if output_schema is not None else 0
        channel_key = (
            channel.topic,
            channel.message_encoding,
            schema_id,
            tuple(sorted(channel.metadata.items())),
        )
        output_channel = self.channels.get(channel_key)
        if output_channel is None:
            output_channel = replace(
                channel, id=len(self.channels) + 1, schema_id=schema_id
            )
            self.channels[channel_key] = output_channel
        return output_schema, output_channel


def _nanoseconds(value: datetime) -> int:
    if value.utcoffset() is None:
        raise ValueError("Episode window boundaries must be timezone-aware")
    delta = value - datetime(1970, 1, 1, tzinfo=timezone.utc)
    return (
        delta.days * 86400 + delta.seconds
    ) * 1_000_000_000 + delta.microseconds * 1000


def _iter_messages(
    store: ObjectStore,
    locations: Sequence[ObjectLocation],
    topics: Sequence[str],
    start: datetime,
    end: datetime,
    decoder_factories: Sequence[DecoderFactory] | None,
    message_ids: _MessageIds | None = None,
) -> Generator[tuple[Schema | None, Channel, Message, Any], None, None]:
    from mcap.exceptions import DecoderNotFoundError
    from mcap.reader import SeekingReader

    start_ns, end_ns = _nanoseconds(start), _nanoseconds(end) + 1
    if decoder_factories is None:
        from foxglove.client.api import DEFAULT_DECODER_FACTORIES

        factories = copy.deepcopy(DEFAULT_DECODER_FACTORIES)
    else:
        factories = decoder_factories

    # MCAP IDs are file-local. Give equivalent schemas/channels the same episode
    # ID so consumers can keep decoder state across split recordings safely.
    ids = message_ids if message_ids is not None else _MessageIds()
    decoders: dict[int, Callable[[bytes], Any]] = {}

    with ExitStack() as cleanup:
        streams = []
        for location in locations:
            file = cleanup.enter_context(store.open(location))
            if not file.seekable():
                raise ValueError("Direct object storage requires seekable MCAP files")
            # MCAP parses summaries and chunk headers with many tiny reads. Bound
            # read-ahead while coalescing these into fewer object-store requests.
            buffered = cleanup.enter_context(
                BufferedReader(cast(RawIOBase, file), buffer_size=64 * 1024)
            )
            reader = SeekingReader(buffered)
            summary = reader.get_summary()
            if summary is not None and summary.statistics is not None:
                if summary.statistics.message_count == 0:
                    continue
            if summary is None or not summary.chunk_indexes:
                raise ValueError(
                    "Direct object storage requires MCAP summary and chunk indexes"
                )
            stream = reader.iter_messages(
                topics=topics, start_time=start_ns, end_time=end_ns
            )
            streams.append(stream)
        for schema, channel, message in heapq.merge(
            *streams, key=lambda item: item[2].log_time
        ):
            output_schema, output_channel = ids.remap(schema, channel)
            decoder = decoders.get(output_channel.id)
            if decoder is None:
                for factory in factories:
                    decoder = factory.decoder_for(
                        output_channel.message_encoding, output_schema
                    )
                    if decoder is not None:
                        decoders[output_channel.id] = decoder
                        break
                else:
                    raise DecoderNotFoundError(
                        "No decoder for message encoding "
                        f"{output_channel.message_encoding!r}, schema {output_schema}"
                    )
            yield (
                output_schema,
                output_channel,
                replace(message, channel_id=output_channel.id),
                decoder(message.data),
            )
