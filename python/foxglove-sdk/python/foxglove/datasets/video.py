"""CPU H.264 decoding for dataset callbacks; requires ``foxglove-sdk[video]``."""

from __future__ import annotations

import base64
import re
from collections.abc import Generator, Iterable, Iterator, Mapping
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from fractions import Fraction
from typing import TYPE_CHECKING, Any

import av
import numpy as np
from mcap.records import Channel, Message, Schema
from numpy.typing import NDArray

from .reader import EpisodeReader

if TYPE_CHECKING:
    from collections.abc import Sequence

    from mcap.decoder import DecoderFactory

_START_CODE = re.compile(b"\x00\x00\x00?\x01")
_VIDEO_SCHEMAS = {
    "foxglove.CompressedVideo",
    "foxglove_msgs/CompressedVideo",
    "foxglove_msgs/msg/CompressedVideo",
}
_MessageTuple = tuple[Schema | None, Channel, Message, Any]


class VideoDecodeError(ValueError):
    """Video data cannot be decoded completely and safely."""


@dataclass(frozen=True)
class _VideoFrame:
    """An RGB image and the metadata of its original compressed message.

    ``image`` is a writable, contiguous uint8 NumPy array of shape ``[H, W, 3]``.
    All timestamps are integer nanoseconds since the Unix epoch. ``timestamp_ns``
    is the video's capture timestamp; ``log_time_ns`` determines episode membership.
    ``frame_id`` identifies the camera's coordinate frame, not a sequence number.
    """

    image: NDArray[np.uint8]
    topic: str
    channel_id: int
    timestamp_ns: int
    log_time_ns: int
    publish_time_ns: int
    frame_id: str


def _nanoseconds(value: datetime) -> int:
    if value.utcoffset() is None:
        raise ValueError("Video window boundaries must be timezone-aware")
    delta = value - datetime(1970, 1, 1, tzinfo=timezone.utc)
    return (
        delta.days * 86400 + delta.seconds
    ) * 1_000_000_000 + delta.microseconds * 1000


def _field(value: Any, name: str) -> Any:
    return value[name] if isinstance(value, Mapping) else getattr(value, name)


def _timestamp(value: Any) -> int:
    if isinstance(value, Mapping):
        seconds, nanos = value["sec"], value["nsec"]
    elif hasattr(value, "seconds"):
        seconds, nanos = value.seconds, value.nanos
    elif hasattr(value, "nanosec"):
        seconds, nanos = value.sec, value.nanosec
    elif hasattr(value, "secs"):
        seconds, nanos = value.secs, value.nsecs
    else:
        seconds, nanos = value.sec, value.nsec
    if not isinstance(seconds, int) or not isinstance(nanos, int):
        raise ValueError(
            "Video timestamps must contain integer seconds and nanoseconds"
        )
    if not 0 <= nanos < 1_000_000_000:
        raise ValueError("Video timestamp nanoseconds must be in [0, 1000000000)")
    return seconds * 1_000_000_000 + nanos


def _nal_types(data: bytes) -> set[int]:
    starts = list(_START_CODE.finditer(data))
    if not starts or any(data[: starts[0].start()]):
        raise VideoDecodeError("Expected H.264 Annex B data with NAL start codes")
    types = set()
    for index, start in enumerate(starts):
        end = starts[index + 1].start() if index + 1 < len(starts) else len(data)
        if start.end() >= end or data[start.end()] & 0x80:
            raise VideoDecodeError("Invalid H.264 NAL unit")
        types.add(data[start.end()] & 0x1F)
    if not types.intersection({1, 5}):
        raise VideoDecodeError("Each CompressedVideo message must contain one image")
    return types


@dataclass(frozen=True)
class _Source:
    timestamp_ns: int
    log_time_ns: int
    publish_time_ns: int
    frame_id: str


class _Decoder:
    def __init__(self, channel: Channel, start: int, end: int) -> None:
        self.channel = channel
        self.start = start
        self.end = end
        self.codec: av.VideoCodecContext | None = None
        self.pending: dict[int, _Source] = {}
        self.sequence = 0

    def decode(self, message: Message, decoded: Any) -> Iterator[_VideoFrame]:
        if _field(decoded, "format") != "h264":
            raise VideoDecodeError("Only H.264 CompressedVideo messages are supported")
        payload = _field(decoded, "data")
        data = (
            base64.b64decode(payload, validate=True)
            if isinstance(payload, str)
            else bytes(payload)
        )
        types = _nal_types(data)
        if self.codec is None:
            if not {5, 7, 8}.issubset(types):
                if message.log_time < self.start:
                    return
                raise VideoDecodeError(
                    "Missing H.264 initialization history (IDR keyframe with SPS/PPS). "
                    "Increase lookback or attach the recording containing the preceding "
                    "keyframe to the episode; no in-window frames can be skipped."
                )
            self.codec = av.CodecContext.create("h264", "r")
            self.codec.options = {"err_detect": "explode"}
            # Dataset workers already provide parallelism; avoid CPU oversubscription.
            self.codec.thread_count = 1

        source = _Source(
            _timestamp(_field(decoded, "timestamp")),
            message.log_time,
            message.publish_time,
            _field(decoded, "frame_id"),
        )
        packet = av.Packet(data)
        # Unique identities survive decoder buffering, even with duplicate capture times.
        packet.pts = packet.dts = self.sequence
        packet.time_base = Fraction(1, 1)
        self.pending[self.sequence] = source
        self.sequence += 1
        frames = self.codec.decode(packet)
        if self.codec.has_b_frames:
            raise VideoDecodeError("B-frames are not supported by CompressedVideo")
        yield from self._frames(frames)

    def _frames(self, frames: Iterable[av.VideoFrame]) -> Iterator[_VideoFrame]:
        for frame in frames:
            if frame.is_corrupt:
                raise VideoDecodeError("Decoder returned a corrupt H.264 frame")
            if frame.pts is None or frame.pts not in self.pending:
                raise VideoDecodeError(
                    "Decoded image cannot be associated with one message"
                )
            source = self.pending.pop(frame.pts)
            if self.start <= source.log_time_ns <= self.end:
                yield _VideoFrame(
                    np.ascontiguousarray(frame.to_ndarray(format="rgb24")),
                    self.channel.topic,
                    self.channel.id,
                    source.timestamp_ns,
                    source.log_time_ns,
                    source.publish_time_ns,
                    source.frame_id,
                )

    def flush(self) -> Iterator[_VideoFrame]:
        if self.codec is not None:
            yield from self._frames(self.codec.decode(None))
            if self.pending:
                raise VideoDecodeError("H.264 messages did not produce one image each")

    def close(self) -> None:
        # PyAV releases AVCodecContext when its Python owner is released.
        self.codec = None
        self.pending.clear()


def _decode(
    messages: Iterator[_MessageTuple], start: int, end: int
) -> Generator[_VideoFrame, None, None]:
    decoders: dict[int, _Decoder] = {}
    previous_time: int | None = None
    try:
        for schema, channel, message, decoded in messages:
            if previous_time is not None and message.log_time < previous_time:
                raise VideoDecodeError("Messages must be supplied in log-time order")
            previous_time = message.log_time
            if message.log_time > end:
                break
            if schema is None or schema.name not in _VIDEO_SCHEMAS:
                continue
            decoder = decoders.get(channel.id)
            if decoder is None:
                decoder = _Decoder(channel, start, end)
                decoders[channel.id] = decoder
            elif decoder.channel != channel:
                raise VideoDecodeError(
                    "Channel identity changed within the video stream"
                )
            try:
                yield from decoder.decode(message, decoded)
            except (
                ValueError,
                TypeError,
                KeyError,
                AttributeError,
                av.FFmpegError,
            ) as error:
                raise VideoDecodeError(
                    f"Video on topic {channel.topic!r}, channel {channel.id}, "
                    f"log time {message.log_time}: {error}"
                ) from error
        for decoder in decoders.values():
            try:
                yield from decoder.flush()
            except (ValueError, av.FFmpegError) as error:
                raise VideoDecodeError(
                    f"Flushing video on topic {decoder.channel.topic!r}, "
                    f"channel {decoder.channel.id}: {error}"
                ) from error
    finally:
        for decoder in decoders.values():
            decoder.close()
        decoders.clear()


@contextmanager
def _decode_h264_messages(
    messages: Iterable[_MessageTuple], *, start_time: datetime, end_time: datetime
) -> Iterator[Iterator[_VideoFrame]]:
    """Decode ordered CompressedVideo messages into RGB frames using CPU PyAV.

    Use as ``with _decode_h264_messages(...) as frames`` inside an episode callback. Each
    invocation creates independent state per MCAP channel, in the consuming worker.
    Non-video schemas are ignored; unsupported video codecs raise VideoDecodeError.
    Protobuf, ROS 1/2, and Foxglove JSON message representations are accepted after
    deserialization. H.264 must be Annex B, one image per message, without B-frames.
    Initialization requires an IDR keyframe containing SPS and PPS NAL units.

    :param messages: MCAP ``(schema, channel, message, decoded_message)`` tuples in
        log-time/decode order from one source, including any necessary lookback.
        Do not subsample packets before decoding or concatenate unrelated sources.
    :param start_time: Inclusive log-time boundary for emitted images. Earlier
        messages initialize the decoder but do not produce output samples.
    :param end_time: Inclusive log-time boundary, matching the streaming API.
        Both boundaries must be timezone-aware.
    :returns: A context manager yielding a streaming iterator of _VideoFrame values.
        Ordering is preserved per channel; buffered frames across channels are not
        guaranteed to arrive in global timestamp order. Normal exhaustion flushes
        delayed images. Context exit closes the iterator and input stream (if
        closable), and releases decoder state, including on errors or cancellation.
    :raises VideoDecodeError: Missing history, malformed/unsupported video, or
        incomplete decoding. Consume to exhaustion to validate the entire window.
    """
    source = iter(messages)
    frames = None
    try:
        start, end = _nanoseconds(start_time), _nanoseconds(end_time)
        if start > end:
            raise ValueError("start_time must be at or before end_time")
        frames = _decode(source, start, end)
        yield frames
    finally:
        try:
            if frames is not None:
                frames.close()
        finally:
            close = getattr(source, "close", None)
            if close is not None:
                close()


def decode_h264(
    episode: EpisodeReader,
    *,
    topic: str | None = None,
    lookback: timedelta = timedelta(seconds=5),
    decoder_factories: Sequence[DecoderFactory] | None = None,
) -> Generator[dict[str, Any], None, None]:
    """Stream decoded H.264 frames from an episode as sample dictionaries.

    Pass directly as ``read_episode=decode_h264``, return it from a callback, or
    create independent iterators with ``decode_h264(episode, topic="/camera")``.
    Each iterator opens its own download on first iteration, with server-side
    topic filtering. Decoder state is local to each invocation and MCAP channel.

    :param episode: Reader supplied to the dataset callback. Its inclusive log-time
        window determines which images are emitted; earlier history only initializes
        decoder state. The reader closes all iterators when the callback finishes,
        fails, or is cancelled, even if some iterators are only partially consumed.
    :param topic: One of the dataset's selected topics. None reads all selected
        topics, ignoring non-video schemas. Multiple camera outputs are not paired.
    :param lookback: History to fetch before the episode, default five seconds.
        Must be nonnegative. Only attached recordings are searched. Insufficient
        initialization history raises VideoDecodeError; increase this budget or
        attach the missing recording rather than dropping in-window frames.
    :param decoder_factories: Optional MCAP message decoders, constructed in the
        worker callback. None uses the client's defaults.
    :returns: Dictionaries with ``image`` (contiguous RGB uint8 [H, W, 3]),
        ``topic``, ``channel_id``, ``frame_id`` (coordinate frame identifier), and
        ``timestamp_ns`` (capture), ``log_time_ns``, ``publish_time_ns`` (integer
        nanoseconds since epoch). Supports CompressedVideo
        H.264 Annex B, one image per message, no B-frames, with an initial IDR/SPS/PPS.
        Output is ordered per channel, not globally across cameras. Consume to
        exhaustion to receive delayed frames and validate completeness. For early
        release inside a callback, call close() or use contextlib.closing().
    """

    def samples() -> Generator[dict[str, Any], None, None]:
        messages = episode.iter_messages(
            topics=None if topic is None else [topic],
            lookback=lookback,
            decoder_factories=decoder_factories,
        )
        with _decode_h264_messages(
            messages, start_time=episode.start_time, end_time=episode.end_time
        ) as frames:
            for frame in frames:
                yield dict(
                    image=frame.image,
                    topic=frame.topic,
                    channel_id=frame.channel_id,
                    timestamp_ns=frame.timestamp_ns,
                    log_time_ns=frame.log_time_ns,
                    publish_time_ns=frame.publish_time_ns,
                    frame_id=frame.frame_id,
                )

    return episode._manage(samples())
