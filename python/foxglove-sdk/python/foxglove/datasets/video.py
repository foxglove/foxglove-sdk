"""CPU H.264 decoding for dataset callbacks; requires ``foxglove-sdk[video]``."""

from __future__ import annotations

import base64
import re
from collections.abc import Generator, Iterable, Iterator, Mapping
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from fractions import Fraction
from typing import TYPE_CHECKING, Any, TypedDict

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


class VideoSample(TypedDict):
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
        raise VideoDecodeError(
            "CompressedVideo message contains no H.264 image slice; "
            "SPS/PPS must be in the same message as the keyframe"
        )
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

    def decode(self, message: Message, decoded: Any) -> Iterator[VideoSample]:
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

    def _frames(self, frames: Iterable[av.VideoFrame]) -> Iterator[VideoSample]:
        for frame in frames:
            if frame.is_corrupt:
                raise VideoDecodeError("Decoder returned a corrupt H.264 frame")
            if frame.pts is None or frame.pts not in self.pending:
                raise VideoDecodeError(
                    "Decoded image cannot be associated with one message"
                )
            source = self.pending.pop(frame.pts)
            if self.start <= source.log_time_ns <= self.end:
                yield VideoSample(
                    image=np.ascontiguousarray(frame.to_ndarray(format="rgb24")),
                    topic=self.channel.topic,
                    channel_id=self.channel.id,
                    timestamp_ns=source.timestamp_ns,
                    log_time_ns=source.log_time_ns,
                    publish_time_ns=source.publish_time_ns,
                    frame_id=source.frame_id,
                )

    def flush(self) -> Iterator[VideoSample]:
        if self.codec is not None:
            yield from self._frames(self.codec.decode(None))
            if self.pending:
                raise VideoDecodeError("H.264 messages did not produce one image each")

    def close(self) -> None:
        # PyAV releases AVCodecContext when its Python owner is released.
        self.codec = None
        self.pending.clear()


def _decode_h264_messages(
    messages: Iterable[_MessageTuple],
    *,
    start_time: datetime,
    end_time: datetime,
    require_video: bool = False,
) -> Generator[VideoSample, None, None]:
    """Decode one ordered stream; close its input and per-channel state on exit."""
    source = iter(messages)
    decoders: dict[int, _Decoder] = {}
    previous_time: int | None = None
    try:
        start, end = _nanoseconds(start_time), _nanoseconds(end_time)
        if start > end:
            raise ValueError("start_time must be at or before end_time")
        for schema, channel, message, decoded in source:
            if previous_time is not None and message.log_time < previous_time:
                raise VideoDecodeError("Messages must be supplied in log-time order")
            previous_time = message.log_time
            if message.log_time > end:
                break
            if schema is None or schema.name not in _VIDEO_SCHEMAS:
                if require_video:
                    raise VideoDecodeError(
                        f"Topic {channel.topic!r} does not contain CompressedVideo messages"
                    )
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
        close = getattr(source, "close", None)
        if close is not None:
            close()


def decode_h264(
    episode: EpisodeReader,
    *,
    topic: str | None = None,
    lookback: timedelta = timedelta(seconds=5),
    decoder_factories: Sequence[DecoderFactory] | None = None,
) -> Generator[VideoSample, None, None]:
    """Yield RGB frames from an episode. Requires ``foxglove-sdk[video]``.

    Use directly as ``read_episode=decode_h264`` with PyTorch or Ray, or call
    inside an episode callback. Each invocation opens one download and maintains
    independent decoder state per MCAP channel. The episode closes its iterators on exit;
    call ``close()`` to release one earlier.

    :param episode: Supplies the inclusive MCAP log-time window for output frames.
    :param topic: One selected dataset topic, or None for all selected topics.
        Non-video schemas are ignored only when topic is None.
    :param lookback: Nonnegative history budget, default five seconds. Searches
        only attached recordings. Missing initialization history raises
        VideoDecodeError. Pre-keyframe slice data is skipped without decoding.
    :param decoder_factories: Optional MCAP deserializers created in the callback.
        None uses the client's defaults.
    :returns: VideoSample dictionaries, ordered per channel, without camera
        synchronization. Consume to exhaustion to receive buffered frames.
    :raises VideoDecodeError: Unsupported or invalid video, or incomplete decoding.
        Supports CompressedVideo H.264 Annex B, one image per message, no B-frames,
        with an initial IDR keyframe containing SPS/PPS.
    """

    messages = episode.iter_messages(
        topics=None if topic is None else [topic],
        lookback=lookback,
        decoder_factories=decoder_factories,
    )
    return episode._manage(
        _decode_h264_messages(
            messages,
            start_time=episode.start_time,
            end_time=episode.end_time,
            require_video=topic is not None,
        )
    )
