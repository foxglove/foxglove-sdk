import base64
import json
import re
from collections.abc import Generator, Iterator
from contextlib import closing
from dataclasses import replace
from datetime import datetime, timedelta, timezone
from pathlib import Path
from types import SimpleNamespace
from typing import Any

import pytest
from foxglove.datasets import EpisodeReader
from foxglove.datasets.reader import _Episode, _plan
from mcap.decoder import DecoderFactory
from mcap.records import Channel, Message, Schema

from .test_datasets import Client

av = pytest.importorskip("av")
np = pytest.importorskip("numpy")

from foxglove.datasets.video import (  # noqa: E402
    VideoDecodeError,
    VideoSample,
    _decode_h264_messages,
    _Decoder,
    decode_h264,
)

START = datetime(2026, 1, 1, tzinfo=timezone.utc)
START_NS = 1767225600_000000000
MessageTuple = tuple[Schema | None, Channel, Message, Any]


def messages(
    fixture: str = "h264", channel_id: int = 1, topic: str = "/camera"
) -> list[MessageTuple]:
    packets = json.loads(
        (Path(__file__).parent / "fixtures" / f"{fixture}.json").read_text()
    )["packets"]
    schema = Schema(
        id=1, name="foxglove.CompressedVideo", encoding="jsonschema", data=b"{}"
    )
    channel = Channel(
        id=channel_id, topic=topic, schema_id=1, message_encoding="json", metadata={}
    )
    result: list[MessageTuple] = []
    for index, packet in enumerate(packets):
        decoded = {
            "data": packet,
            "format": "h264",
            "timestamp": {"sec": 1700000000, "nsec": 123456789},
            "frame_id": f"camera-{channel_id}",
        }
        message = Message(
            channel_id=channel_id,
            log_time=START_NS + index * 1_000_000_000,
            publish_time=START_NS + index * 1_000_000_000 - 123,
            sequence=index,
            data=json.dumps(decoded).encode(),
        )
        result.append((schema, channel, message, decoded))
    return result


def decode(rows: list[MessageTuple], start: int = 0, end: int = 7) -> list[VideoSample]:
    return list(
        _decode_h264_messages(
            rows,
            start_time=START + timedelta(seconds=start),
            end_time=START + timedelta(seconds=end),
        )
    )


def test_decodes_interdependent_frames_and_preserves_duplicate_timestamps() -> None:
    _assert_full_decode(decode(messages()))


def _assert_full_decode(frames: list[VideoSample]) -> None:
    assert len(frames) == 8
    for index, frame in enumerate(frames):
        assert frame["image"].shape == (32, 48, 3)
        assert frame["image"].dtype == np.uint8
        assert frame["image"].flags.c_contiguous and frame["image"].flags.writeable
        np.testing.assert_allclose(
            frame["image"][0, 0], [40 + index * 15, 100, 180 - index * 10], atol=5
        )
        assert frame["timestamp_ns"] == 1700000000_123456789
        assert frame["log_time_ns"] == START_NS + index * 1_000_000_000
        assert frame["publish_time_ns"] == frame["log_time_ns"] - 123
        assert (frame["channel_id"], frame["topic"], frame["frame_id"]) == (
            1,
            "/camera",
            "camera-1",
        )


@pytest.mark.parametrize("offset, start, end", [(0, 2, 5), (0, 3, 3), (1, 5, 7)])
def test_lookback_matches_full_decode(offset: int, start: int, end: int) -> None:
    window = decode(messages()[offset:], start=start, end=end)
    expected = decode(messages())[start : end + 1]
    assert len(window) == len(expected)
    for actual, reference in zip(window, expected):
        np.testing.assert_array_equal(actual["image"], reference["image"])
        assert actual["log_time_ns"] == reference["log_time_ns"]


def test_missing_history_errors_before_skipping_in_window_frames() -> None:
    with pytest.raises(VideoDecodeError, match="/camera.*Increase lookback"):
        decode(messages()[1:], start=2)


def test_keyframe_without_parameter_sets_is_not_accepted() -> None:
    rows = messages()
    units = re.split(b"\x00\x00\x00?\x01", base64.b64decode(rows[0][3]["data"]))
    rows[0][3]["data"] = b"".join(
        b"\x00\x00\x00\x01" + unit for unit in units if unit and unit[0] & 31 == 5
    )
    with pytest.raises(VideoDecodeError, match="SPS/PPS"):
        decode(rows)


@pytest.mark.parametrize("same_topic", [False, True])
def test_independent_state_for_interleaved_channels(same_topic: bool) -> None:
    first = messages()
    second = messages(channel_id=2, topic="/camera" if same_topic else "/other")
    interleaved = [row for pair in zip(first, second) for row in pair]
    frames = decode(interleaved)
    assert len(frames) == 16
    for channel_id in (1, 2):
        selected = [frame for frame in frames if frame["channel_id"] == channel_id]
        for actual, expected in zip(selected, decode(first)):
            np.testing.assert_array_equal(actual["image"], expected["image"])
        assert {f["frame_id"] for f in selected} == {f"camera-{channel_id}"}


def test_delayed_frames_are_flushed_with_original_metadata(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    original = _Decoder.decode

    def delayed(
        self: _Decoder, message: Message, decoded: Any
    ) -> Iterator[VideoSample]:
        if self.codec is None:
            self.codec = av.CodecContext.create("h264", "r")
            self.codec.thread_count = 3
            self.codec.thread_type = "FRAME"
        yield from original(self, message, decoded)

    monkeypatch.setattr(_Decoder, "decode", delayed)
    flush_counts = []
    original_flush = _Decoder.flush

    def flush(self: _Decoder) -> Iterator[VideoSample]:
        remaining = list(original_flush(self))
        flush_counts.append(len(remaining))
        yield from remaining

    monkeypatch.setattr(_Decoder, "flush", flush)
    _assert_full_decode(decode(messages()))
    assert sum(flush_counts) > 0


@pytest.fixture
def closed_decoders(monkeypatch: pytest.MonkeyPatch) -> list[_Decoder]:
    states = []
    original = _Decoder.close

    def close(self: _Decoder) -> None:
        original(self)
        states.append(self)

    monkeypatch.setattr(_Decoder, "close", close)
    return states


@pytest.mark.parametrize(
    "outcome", ["complete", "cancel", "error", "decode_error", "consumer_error"]
)
def test_closes_source_and_decoder_even_if_iterator_is_retained(
    closed_decoders: list[_Decoder], outcome: str
) -> None:
    closed = []

    def source() -> Generator[MessageTuple, None, None]:
        try:
            for index, row in enumerate(messages()):
                if outcome == "error" and index == 2:
                    raise RuntimeError("source error")
                if outcome == "decode_error" and index == 2:
                    row[3]["data"] = b"bad"
                yield row
        finally:
            closed.append(True)

    retained = source()
    retained_frames = []

    def consume() -> None:
        with closing(
            _decode_h264_messages(
                retained, start_time=START, end_time=START + timedelta(seconds=7)
            )
        ) as frames:
            retained_frames.append(frames)
            if outcome in {"complete", "error", "decode_error"}:
                assert len(list(frames)) == 8
            else:
                next(frames)
                if outcome == "consumer_error":
                    raise RuntimeError("consumer error")

    if outcome in {"error", "consumer_error"}:
        with pytest.raises(RuntimeError, match="error"):
            consume()
    elif outcome == "decode_error":
        with pytest.raises(VideoDecodeError):
            consume()
    else:
        consume()
    assert closed == [True]
    assert closed_decoders
    assert all(state.codec is None and not state.pending for state in closed_decoders)
    assert list(retained_frames[0]) == []


@pytest.mark.parametrize("representation", ["protobuf", "ros1", "ros2", "sdk"])
def test_attribute_message_representations(representation: str) -> None:
    rows = messages()
    fields = {
        "protobuf": {"seconds": 1700000000, "nanos": 123456789},
        "ros1": {"secs": 1700000000, "nsecs": 123456789},
        "ros2": {"sec": 1700000000, "nanosec": 123456789},
        "sdk": {"sec": 1700000000, "nsec": 123456789},
    }
    converted = []
    for schema, channel, message, decoded in rows:
        converted.append(
            (
                schema,
                channel,
                message,
                SimpleNamespace(
                    data=base64.b64decode(decoded["data"]),
                    format="h264",
                    timestamp=SimpleNamespace(**fields[representation]),
                    frame_id="camera-1",
                ),
            )
        )
    assert [f["timestamp_ns"] for f in decode(converted)] == [1700000000_123456789] * 8


@pytest.mark.parametrize("lookback", [False, True])
@pytest.mark.parametrize(
    "payload", [b"bad", b"", b"\x00\x00\x01", b"\x00\x00\x01\x67", "invalid base64!"]
)
def test_rejects_malformed_payloads(payload: bytes | str, lookback: bool) -> None:
    rows = messages()[1:] if lookback else messages()
    rows[0][3]["data"] = payload
    with pytest.raises(VideoDecodeError, match="/camera"):
        decode(rows, start=5 if lookback else 0)


def test_rejects_unsupported_codec_and_b_frames() -> None:
    rows = messages()
    rows[0][3]["format"] = "h265"
    with pytest.raises(VideoDecodeError, match="Only H.264"):
        decode(rows)
    with pytest.raises(VideoDecodeError, match="B-frames"):
        decode(messages("h264_b_frames"))


def test_rejects_out_of_order_messages() -> None:
    rows = messages()
    rows[1], rows[2] = rows[2], rows[1]
    with pytest.raises(VideoDecodeError, match="log-time order"):
        decode(rows)


def test_ignores_non_video_and_empty_streams() -> None:
    rows = messages()
    schema, channel, message, decoded = rows[0]
    assert schema is not None
    rows[0] = (
        replace(schema, name="foxglove.CompressedImage"),
        channel,
        message,
        decoded,
    )
    assert decode(rows[:1]) == []
    assert decode([]) == []


class VideoClient(Client):
    def iter_messages(
        self,
        *,
        episode_id: str,
        topics: list[str],
        decoder_factories: list[DecoderFactory] | None = None,
        start: datetime | None = None,
        end: datetime | None = None,
    ) -> Generator[MessageTuple, None, None]:
        self.calls.append((episode_id, topics))
        self.time_ranges.append((start, end))
        try:
            rows = [
                row
                for channel_id, topic in enumerate(topics, 1)
                for row in messages(topic=topic, channel_id=channel_id)
            ]
            yield from sorted(rows, key=lambda row: row[2].log_time)
        finally:
            self.closed.append(episode_id)


def video_reader(
    *topics: str, start: int = 0, end: int = 7
) -> tuple[EpisodeReader, VideoClient]:
    client = VideoClient()
    episode = _Episode(
        "a", START + timedelta(seconds=start), START + timedelta(seconds=end), {}
    )
    return EpisodeReader(episode, topics or ("/camera",), client), client


def video_samples(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
    for frame in decode_h264(episode):
        yield {**frame, "episode_id": episode.id}


def test_rejects_invalid_window_and_naive_times() -> None:
    with pytest.raises(ValueError, match="at or before"):
        decode([], start=3, end=2)
    with pytest.raises(ValueError, match="timezone-aware"):
        list(_decode_h264_messages([], start_time=datetime(2026, 1, 1), end_time=START))


def test_rejects_multiple_images_in_one_message() -> None:
    rows = messages()
    rows[0][3]["data"] = base64.b64decode(rows[0][3]["data"]) + base64.b64decode(
        rows[1][3]["data"]
    )
    with pytest.raises(VideoDecodeError):
        decode(rows)


def test_episode_decode_infers_window_and_returns_complete_metadata() -> None:
    reader, client = video_reader(start=2, end=5)
    frames = decode_h264(reader)
    assert client.calls == []
    rows = list(frames)
    assert len(rows) == 4
    assert set(rows[0]) == {
        "image",
        "topic",
        "channel_id",
        "timestamp_ns",
        "log_time_ns",
        "publish_time_ns",
        "frame_id",
    }
    for row, expected in zip(rows, decode(messages(), start=2, end=5)):
        np.testing.assert_array_equal(row["image"], expected["image"])
        assert row["log_time_ns"] == expected["log_time_ns"]
        assert row["timestamp_ns"] == expected["timestamp_ns"]
    assert client.time_ranges == [
        (reader.start_time - timedelta(seconds=5), reader.end_time)
    ]
    assert client.closed == [reader.id]
    assert not reader._streams


def test_independent_topic_decoders_request_only_their_camera() -> None:
    topics = ("/camera/front", "/camera/wrist")
    reader, client = video_reader(*topics)
    front = decode_h264(reader, topic="/camera/front", lookback=timedelta(seconds=2))
    wrist = decode_h264(reader, topic="/camera/wrist", lookback=timedelta(seconds=8))
    assert client.calls == []
    assert next(front)["topic"] == "/camera/front"
    assert next(wrist)["topic"] == "/camera/wrist"
    assert len(list(front)) == 7
    assert len(list(wrist)) == 7
    assert client.calls == [
        (reader.id, ["/camera/front"]),
        (reader.id, ["/camera/wrist"]),
    ]
    assert client.time_ranges == [
        (START - timedelta(seconds=n), reader.end_time) for n in (2, 8)
    ]
    assert not reader._streams


@pytest.mark.parametrize("outcome", ["complete", "cancel", "error"])
def test_episode_owns_partially_consumed_video_iterators(
    closed_decoders: list[_Decoder], outcome: str
) -> None:
    client = VideoClient()
    plan = _plan("dataset", 7, ["/camera/front", "/camera/wrist"], lambda: client)
    retained = []

    def sample(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
        front = decode_h264(episode, topic="/camera/front")
        wrist = decode_h264(episode, topic="/camera/wrist")
        unused = decode_h264(episode)
        retained.extend([front, wrist, unused])
        next(front)
        next(wrist)
        if outcome == "error":
            raise ValueError("callback failed")
        yield {"id": episode.id}

    stream = plan.read(plan.episodes[:1], sample)
    if outcome == "error":
        with pytest.raises(RuntimeError, match="episode a"):
            next(stream)
    elif outcome == "cancel":
        next(stream)
        stream.close()
    else:
        assert list(stream) == [{"id": "a"}]
    assert len(closed_decoders) == 2
    assert all(state.codec is None and not state.pending for state in closed_decoders)
    assert client.closed == ["a", "a"]
    assert all(list(iterator) == [] for iterator in retained)


@pytest.mark.parametrize(
    "topic, lookback, match",
    [
        ("/outside", timedelta(seconds=5), "subset"),
        ("/camera", timedelta(seconds=-1), "nonnegative"),
    ],
)
def test_episode_decoder_rejects_invalid_requests_before_downloading(
    topic: str, lookback: timedelta, match: str
) -> None:
    reader, client = video_reader()
    with pytest.raises(ValueError, match=match):
        list(decode_h264(reader, topic=topic, lookback=lookback))
    assert not client.calls
    assert not reader._streams
    reader._close()
    with pytest.raises(RuntimeError, match="closed"):
        decode_h264(reader)


def test_episode_decoder_reads_all_selected_topics_in_one_request() -> None:
    topics = ("/camera/front", "/camera/wrist")
    reader, client = video_reader(*topics)
    rows = list(decode_h264(reader, lookback=timedelta(0)))
    assert len(rows) == 16
    assert {row["topic"] for row in rows} == set(topics)
    assert client.calls == [(reader.id, list(topics))]
    assert client.time_ranges == [(None, None)]
    assert not reader._streams


@pytest.mark.parametrize("initialized, start", [(False, 5), (True, 5), (True, 0)])
def test_corrupt_lookback_slice_is_checked_only_after_initialization(
    initialized: bool,
    start: int,
) -> None:
    rows = messages()
    # Valid Annex B framing and NAL header, but invalid slice data.
    rows[1][3]["data"] = b"\x00\x00\x00\x01\x61\x00"
    if initialized:
        with pytest.raises(VideoDecodeError, match="/camera"):
            decode(rows, start=start)
    else:
        window = decode(rows[1:], start=5)
        expected = decode(messages(), start=5)
        assert len(window) == len(expected) == 3
        for actual, reference in zip(window, expected):
            np.testing.assert_array_equal(actual["image"], reference["image"])
            assert actual["log_time_ns"] == reference["log_time_ns"]


@pytest.mark.parametrize(
    "schema_name", [None, "foxglove.CompressedImage", "JointState"]
)
def test_explicit_non_video_topic_raises_and_closes_stream(
    monkeypatch: pytest.MonkeyPatch, schema_name: str | None
) -> None:
    closed = []

    def non_video(
        self: VideoClient, **kwargs: Any
    ) -> Generator[MessageTuple, None, None]:
        schema, channel, message, decoded = messages()[0]
        assert schema is not None
        try:
            yield (
                None if schema_name is None else replace(schema, name=schema_name),
                channel,
                message,
                decoded,
            )
        finally:
            closed.append(True)

    monkeypatch.setattr(VideoClient, "iter_messages", non_video)
    reader, client = video_reader()
    with pytest.raises(
        VideoDecodeError, match="/camera.*does not contain CompressedVideo"
    ):
        list(decode_h264(reader, topic="/camera"))
    assert not reader._streams
    assert list(decode_h264(reader)) == []
    assert closed == [True, True]


def test_fractional_window_filters_nanosecond_message_times_exactly() -> None:
    rows = messages()
    # Episode boundaries are datetimes; message timestamps retain nanosecond precision.
    times = [
        0,
        999_999,
        1_000_000,
        1_000_001,
        1_999_999,
        2_000_000,
        2_000_001,
        3_000_000,
    ]
    shifted = [
        (schema, channel, replace(message, log_time=START_NS + nanos), decoded)
        for (schema, channel, message, decoded), nanos in zip(rows, times)
    ]
    with closing(
        _decode_h264_messages(
            shifted,
            start_time=START + timedelta(milliseconds=1),
            end_time=START + timedelta(milliseconds=2),
        )
    ) as frames:
        assert [frame["log_time_ns"] for frame in frames] == [
            START_NS + nanos for nanos in times[2:6]
        ]
