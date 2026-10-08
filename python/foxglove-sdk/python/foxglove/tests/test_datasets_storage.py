import io
import json
import threading
from collections.abc import Callable, Iterator
from datetime import datetime, timedelta, timezone
from typing import IO, Any
from unittest.mock import MagicMock

import pytest
from foxglove.datasets.reader import EpisodeReader, _Episode, _plan
from foxglove.datasets.storage import ObjectLocation, _iter_messages
from mcap.decoder import DecoderFactory
from mcap.exceptions import DecoderNotFoundError
from mcap.records import Schema
from mcap.writer import CompressionType, Writer

START = datetime(2026, 1, 1, tzinfo=timezone.utc)
START_NS = 1767225600 * 1_000_000_000
LOCATION = ObjectLocation("bucket", "prefix/run.mcap")


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
) -> bytes:
    output = io.BytesIO()
    writer = Writer(
        output,
        chunk_size=1,
        compression=CompressionType.NONE,
        use_chunking=indexed,
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


def test_indexed_sparse_reads_filter_topics_time_and_include_end() -> None:
    data = mcap_bytes(
        [
            (topic, index * 1_000_000_000, {"value": index, "padding": "x" * 16384})
            for index in range(100)
            for topic in ("/selected", "/other")
        ]
    )
    store = Store({LOCATION: data})
    start = START + timedelta(seconds=50)
    result = list(_iter_messages(store, [LOCATION], ["/selected"], start, start, None))

    assert [
        (channel.topic, message.log_time, decoded["value"])
        for _, channel, message, decoded in result
    ] == [("/selected", START_NS + 50_000_000_000, 50)]
    file = store.files[0]
    assert file.closed
    assert file.read_bytes < len(data) // 10
    assert len(file.ranges) < 10
    dense_store = Store({LOCATION: data})
    dense = list(
        _iter_messages(
            dense_store,
            [LOCATION],
            ["/selected", "/other"],
            START,
            START + timedelta(seconds=99),
            None,
        )
    )
    assert len(dense) == 200
    assert file.read_bytes < dense_store.files[0].read_bytes // 10
    assert len(file.ranges) < len(dense_store.files[0].ranges)


def test_split_recordings_merge_globally_and_remap_colliding_ids() -> None:
    second = ObjectLocation("bucket", "prefix/second.mcap")
    third = ObjectLocation("bucket", "prefix/third.mcap")
    store = Store(
        {
            LOCATION: mcap_bytes([("/one", 2, 2), ("/one", 4, 4)]),
            second: mcap_bytes([("/two", 1, 1), ("/two", 3, 3)], schema_name="Other"),
            third: mcap_bytes([("/one", 5, 5)]),
        }
    )
    result = list(
        _iter_messages(
            store,
            [LOCATION, second, third],
            ["/one", "/two"],
            START,
            START + timedelta(microseconds=1),
            None,
        )
    )

    assert [decoded for _, _, _, decoded in result] == [1, 2, 3, 4, 5]
    assert [message.log_time for _, _, message, _ in result] == sorted(
        message.log_time for _, _, message, _ in result
    )
    schemas = {schema.name: schema.id for schema, _, _, _ in result if schema}
    assert len(set(schemas.values())) == 2
    channels = {channel.topic: channel.id for _, channel, _, _ in result}
    assert len(set(channels.values())) == 2
    for schema, channel, message, _ in result:
        assert schema is not None
        assert channel.schema_id == schemas[schema.name]
        assert message.channel_id == channels[channel.topic]
    assert result[1][0] is result[4][0]
    assert result[1][1] is result[4][1]
    assert all(file.closed for file in store.files)


def test_lookback_and_repeated_streams_open_fresh_files() -> None:
    store = Store(
        {
            LOCATION: mcap_bytes(
                [
                    ("/selected", 0, "history"),
                    ("/selected", 1_000_000_000, "start"),
                    ("/selected", 2_000_000_000, "end"),
                    ("/selected", 3_000_000_000, "after"),
                ]
            )
        }
    )
    episode = _Episode(
        "episode",
        START + timedelta(seconds=1),
        START + timedelta(seconds=2),
        {},
        (LOCATION,),
    )
    reader = EpisodeReader(episode, ("/selected",), None, store)

    assert [decoded for _, _, _, decoded in reader.iter_messages()] == ["start", "end"]
    assert [
        decoded
        for _, _, _, decoded in reader.iter_messages(lookback=timedelta(seconds=1))
    ] == ["history", "start", "end"]
    assert len(store.files) == 2
    assert all(file.closed for file in store.files)
    reader._close()


def test_cancellation_closes_all_split_recordings() -> None:
    second = ObjectLocation("bucket", "second")
    data = mcap_bytes([("/selected", 0, 1), ("/selected", 1, 2)])
    store = Store({LOCATION: data, second: data})
    episode = _Episode(
        "episode", START, START + timedelta(seconds=1), {}, (LOCATION, second)
    )
    reader = EpisodeReader(episode, ("/selected",), None, store)
    stream = reader.iter_messages()
    next(stream)
    assert len(store.files) == 2
    assert not any(file.closed for file in store.files)

    reader._close()

    assert all(file.closed for file in store.files)
    assert list(stream) == []


def test_failed_open_closes_previously_opened_recordings() -> None:
    second = ObjectLocation("bucket", "missing")
    store = Store({LOCATION: mcap_bytes([("/selected", 0, 1)])})
    with pytest.raises(KeyError):
        list(
            _iter_messages(store, [LOCATION, second], ["/selected"], START, START, None)
        )
    assert store.files[0].closed


@pytest.mark.parametrize("indexed,seekable", [(False, True), (True, False)])
def test_direct_reads_reject_unsupported_mcap_handles(
    indexed: bool, seekable: bool
) -> None:
    store = Store(
        {LOCATION: mcap_bytes([("/selected", 0, 1)], indexed=indexed)},
        seekable=seekable,
    )
    with pytest.raises(ValueError, match="chunk indexes" if seekable else "seekable"):
        list(_iter_messages(store, [LOCATION], ["/selected"], START, START, None))
    assert store.files[0].closed


def test_empty_indexed_recording_produces_no_messages() -> None:
    store = Store({LOCATION: mcap_bytes([])})
    assert (
        list(_iter_messages(store, [LOCATION], ["/selected"], START, START, None)) == []
    )
    assert store.files[0].closed


class CustomDecoder(DecoderFactory):
    def decoder_for(
        self, message_encoding: str, schema: Schema | None
    ) -> Callable[[bytes], Any] | None:
        if message_encoding != "custom":
            return None
        assert schema is not None
        return lambda payload: {
            "schema": schema.name,
            "value": json.loads(payload) + 10,
        }


def test_custom_decoders_use_their_own_file_schemas() -> None:
    second = ObjectLocation("bucket", "second")
    store = Store(
        {
            LOCATION: mcap_bytes([("/selected", 0, 1)], encoding="custom"),
            second: mcap_bytes(
                [("/selected", 1, 2)], encoding="custom", schema_name="Other"
            ),
        }
    )
    result = list(
        _iter_messages(
            store,
            [LOCATION, second],
            ["/selected"],
            START,
            START + timedelta(microseconds=1),
            [CustomDecoder()],
        )
    )
    assert [decoded for _, _, _, decoded in result] == [
        {"schema": "Measurement", "value": 11},
        {"schema": "Other", "value": 12},
    ]
    assert all(file.closed for file in store.files)


def test_decoder_failure_closes_storage_file() -> None:
    store = Store({LOCATION: mcap_bytes([("/selected", 0, 1)], encoding="custom")})
    with pytest.raises(DecoderNotFoundError):
        list(_iter_messages(store, [LOCATION], ["/selected"], START, START, []))
    assert store.files[0].closed


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


def test_direct_plan_maps_azure_location_and_skips_worker_api_client() -> None:
    location = ObjectLocation("container", "prefix/run.mcap", "account")
    recording = {
        "id": "recording",
        "available": True,
        "location": {
            "bucket": location.bucket,
            "path": location.path,
            "azureStorageAccountName": location.azure_storage_account_name,
        },
    }
    client = plan_client([recording, recording])
    client_factory = MagicMock(return_value=client)
    store = Store({location: mcap_bytes([("/selected", 0, 1)])})
    store_factory = MagicMock(return_value=store)
    plan = _plan("dataset", 7, ["/selected"], client_factory, store_factory)
    assert plan.episodes[0].locations == (location,)
    store_factory.assert_not_called()
    client.get_dataset_version_episodes.assert_called_once_with(
        dataset_id="dataset",
        version_number=7,
        limit=2000,
        include_recordings=True,
    )

    def samples(reader: EpisodeReader) -> Iterator[Any]:
        for _, _, _, decoded in reader.iter_messages():
            yield decoded

    assert list(plan.read(plan.episodes, samples)) == [1]
    client_factory.assert_called_once()
    store_factory.assert_called_once()
    client.iter_messages.assert_not_called()
    assert store.locations == [location]
    assert store.files[0].closed


@pytest.mark.parametrize(
    "recordings",
    [
        None,
        [],
        [{"id": "recording", "available": True, "location": None}],
        [
            {
                "id": "recording",
                "available": False,
                "location": {"bucket": "b", "path": "p"},
            }
        ],
        [{"id": "recording", "available": True, "location": {"bucket": "b"}}],
    ],
)
def test_direct_plan_rejects_missing_locations_before_opening_storage(
    recordings: Any,
) -> None:
    client = plan_client(recordings)
    store_factory = MagicMock()
    with pytest.raises(ValueError, match="location|available"):
        _plan("dataset", 7, ["/selected"], lambda: client, store_factory)
    store_factory.assert_not_called()


def test_stateful_custom_decoder_runs_in_global_order_and_is_not_copied() -> None:
    class StatefulDecoder(DecoderFactory):
        def __init__(self) -> None:
            self.lock = threading.Lock()
            self.values: list[int] = []
            self.creations = 0

        def decoder_for(
            self, message_encoding: str, schema: Schema | None
        ) -> Callable[[bytes], Any] | None:
            assert message_encoding == "custom"
            self.creations += 1
            total = 0

            def decode(payload: bytes) -> int:
                nonlocal total
                with self.lock:
                    value = json.loads(payload)
                    self.values.append(value)
                    total += value
                    return total

            return decode

    second = ObjectLocation("bucket", "second")
    store = Store(
        {
            LOCATION: mcap_bytes(
                [("/selected", 2, 2), ("/selected", 4, 4)], encoding="custom"
            ),
            second: mcap_bytes(
                [("/selected", 1, 1), ("/selected", 3, 3)], encoding="custom"
            ),
        }
    )
    factory = StatefulDecoder()
    stream = _iter_messages(
        store,
        [LOCATION, second],
        ["/selected"],
        START,
        START + timedelta(microseconds=1),
        [factory],
    )

    assert next(stream)[3] == 1
    assert factory.values == [1]
    assert [decoded for _, _, _, decoded in stream] == [3, 6, 10]
    assert factory.values == [1, 2, 3, 4]
    assert factory.creations == 1
    assert all(file.closed for file in store.files)


def test_direct_time_filter_preserves_nanosecond_precision() -> None:
    store = Store(
        {
            LOCATION: mcap_bytes(
                [
                    ("/selected", offset, offset)
                    for offset in (999, 1000, 1999, 2000, 2001)
                ]
            )
        }
    )
    result = list(
        _iter_messages(
            store,
            [LOCATION],
            ["/selected"],
            START + timedelta(microseconds=1),
            START + timedelta(microseconds=2),
            None,
        )
    )
    assert [decoded for _, _, _, decoded in result] == [1000, 1999, 2000]


def test_direct_time_filter_rejects_naive_boundaries_before_opening_files() -> None:
    store = Store({})
    with pytest.raises(ValueError, match="timezone-aware"):
        list(
            _iter_messages(
                store,
                [LOCATION],
                ["/selected"],
                START.replace(tzinfo=None),
                START,
                None,
            )
        )
    assert store.locations == []


@pytest.mark.parametrize("concurrent_streams", [False, True])
def test_episode_ids_stay_consistent_across_topic_streams(
    concurrent_streams: bool,
) -> None:
    class CachingDecoder(DecoderFactory):
        def __init__(self) -> None:
            self.decoders: dict[int, Callable[[bytes], Any]] = {}

        def decoder_for(
            self, message_encoding: str, schema: Schema | None
        ) -> Callable[[bytes], Any] | None:
            assert message_encoding == "custom"
            assert schema is not None
            if schema.id not in self.decoders:
                self.decoders[schema.id] = lambda payload: (
                    schema.name,
                    json.loads(payload),
                )
            return self.decoders[schema.id]

    second = ObjectLocation("bucket", "second")
    store = Store(
        {
            LOCATION: mcap_bytes(
                [("/a", 0, 1), ("/a", 1, 2)], encoding="custom", schema_name="SchemaA"
            ),
            second: mcap_bytes(
                [("/b", 0, 3), ("/b", 1, 4)], encoding="custom", schema_name="SchemaB"
            ),
        }
    )
    episode = _Episode(
        "episode", START, START + timedelta(microseconds=1), {}, (LOCATION, second)
    )
    reader = EpisodeReader(episode, ("/a", "/b"), None, store)
    factory = CachingDecoder()
    stream_a = reader.iter_messages(topics=["/a"], decoder_factories=[factory])
    stream_b = reader.iter_messages(topics=["/b"], decoder_factories=[factory])
    if concurrent_streams:
        first_a = next(stream_a)
        first_b = next(stream_b)
        result_a = [first_a, *stream_a]
        result_b = [first_b, *stream_b]
    else:
        result_a = list(stream_a)
        result_b = list(stream_b)
    repeated_a = list(reader.iter_messages(topics=["/a"], decoder_factories=[factory]))

    assert [decoded for _, _, _, decoded in result_a] == [
        ("SchemaA", 1),
        ("SchemaA", 2),
    ]
    assert [decoded for _, _, _, decoded in result_b] == [
        ("SchemaB", 3),
        ("SchemaB", 4),
    ]
    assert repeated_a == result_a
    schema_a, channel_a, _, _ = result_a[0]
    schema_b, channel_b, _, _ = result_b[0]
    assert schema_a is not None and schema_b is not None
    assert schema_a.id != schema_b.id
    assert channel_a.id != channel_b.id
    assert channel_a.schema_id == schema_a.id
    assert channel_b.schema_id == schema_b.id
    assert repeated_a[0][0] is schema_a
    assert repeated_a[0][1] is channel_a
    assert set(factory.decoders) == {schema_a.id, schema_b.id}
    assert all(file.closed for file in store.files)
    reader._close()
