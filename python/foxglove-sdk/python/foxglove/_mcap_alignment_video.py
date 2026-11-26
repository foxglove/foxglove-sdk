"""Decode packet-ordered Foxglove video to standalone PNG images."""

from __future__ import annotations

import re
from functools import cache
from io import BytesIO
from typing import TYPE_CHECKING, Any, cast

from foxglove.messages import CompressedImage

if TYPE_CHECKING:
    from av.video.codeccontext import VideoCodecContext
    from mcap.records import Channel, Schema
    from mcap.writer import Writer


_VIDEO_SCHEMA = "foxglove.CompressedVideo"
_CODECS = {"h264": "h264", "h265": "hevc", "vp9": "vp9", "av1": "libdav1d"}
_VIDEO_SCHEMAS = {
    _VIDEO_SCHEMA,
    "foxglove.Compressed_Video",
    "foxglove_msgs/CompressedVideo",
    "foxglove_msgs/msg/CompressedVideo",
}
_RAW_VIDEO_ENCODINGS = _CODECS.keys() | {"hevc", "avc"}
_ANNEX_B_START = re.compile(rb"\x00{2,}\x01")


def is_video(schema: Schema | None, channel: Channel) -> bool:
    """Recognize a video schema before checking its message encoding."""
    return channel.message_encoding.lower() in _RAW_VIDEO_ENCODINGS or (
        schema is not None and schema.name in _VIDEO_SCHEMAS
    )


@cache
def _image_class() -> type[Any]:
    from google.protobuf import descriptor_pb2, message_factory

    schema = CompressedImage.get_schema()
    descriptors = descriptor_pb2.FileDescriptorSet.FromString(schema.data)
    return cast(type[Any], message_factory.GetMessages(descriptors.file)[schema.name])


def register_image_schema(writer: Writer) -> int:
    """Register the SDK's CompressedImage schema."""
    schema = CompressedImage.get_schema()
    return int(writer.register_schema(schema.name, schema.encoding, schema.data))


class VideoDecoder:
    """Decode one channel in physical order, including unselected dependencies."""

    def __init__(self, schema: Schema, channel: Channel) -> None:
        if (
            schema.name != _VIDEO_SCHEMA
            or schema.encoding != "protobuf"
            or channel.message_encoding != "protobuf"
        ):
            raise ValueError(
                f"{channel.topic}: expected a Protobuf foxglove.CompressedVideo channel"
            )

        import av
        from mcap_protobuf.decoder import DecoderFactory

        self._av = av
        decode_envelope = DecoderFactory().decoder_for("protobuf", schema)
        if decode_envelope is None:
            raise ValueError(f"{channel.topic}: missing CompressedVideo descriptor")
        self._decode_envelope = decode_envelope
        self._image_class = _image_class()
        self._topic = channel.topic
        self._codec: VideoCodecContext | None = None
        self._format = None
        self._packet_count = 0

    def feed(self, data: bytes, *, selected: bool) -> bytes | None:
        """Decode one source packet and encode a PNG only if it was selected."""
        video = self._decode_envelope(data)
        codec = self._codec
        if codec is None:
            codec_name = _CODECS.get(video.format)
            if codec_name is None:
                raise ValueError(
                    f"{self._topic}: unsupported video format {video.format!r}; "
                    "expected H.264, H.265, VP9, or AV1"
                )
            try:
                codec = cast(
                    "VideoCodecContext", self._av.CodecContext.create(codec_name, "r")
                )
            except ValueError as error:
                raise ValueError(
                    f"{self._topic}: PyAV has no {codec_name} video decoder"
                ) from error
            codec.thread_count = 1
            codec.options = {"err_detect": "explode"}
            self._codec = codec
            self._format = video.format
        if video.format != self._format:
            raise ValueError(f"{self._topic}: video format changed within the channel")
        if not video.data:
            raise ValueError(f"{self._topic}: empty video packet")
        if video.format in ("h264", "h265") and not _ANNEX_B_START.match(video.data):
            raise ValueError(f"{self._topic}: expected an Annex B video packet")

        packet = self._av.Packet(video.data)
        packet.pts = self._packet_count
        try:
            frames = codec.decode(packet)
        except self._av.error.FFmpegError as error:
            raise ValueError(f"{self._topic}: video decode failed: {error}") from error
        if len(frames) != 1 or frames[0].pts != self._packet_count:
            raise ValueError(
                f"{self._topic}: expected one immediately decoded frame per packet; "
                "check the initial keyframe, packet boundaries, and frame ordering"
            )
        frame = frames[0]
        if frame.is_corrupt or frame.pict_type == self._av.video.frame.PictureType.B:
            raise ValueError(
                f"{self._topic}: corrupt frames and B-frames are unsupported"
            )
        if self._packet_count == 0 and not frame.key_frame:
            raise ValueError(f"{self._topic}: the stream must begin with a keyframe")
        self._packet_count += 1
        if not selected:
            return None

        pixels = BytesIO()
        frame.to_image().save(pixels, format="PNG", compress_level=1)
        image = self._image_class(
            data=pixels.getvalue(), format="png", frame_id=video.frame_id
        )
        if video.HasField("timestamp"):
            image.timestamp.ParseFromString(video.timestamp.SerializeToString())
        return bytes(image.SerializeToString())

    def finish(self) -> None:
        """Verify the stream ends without a delayed frame."""
        if self._codec is None:
            return
        try:
            frames = self._codec.decode(None)
        except self._av.error.FFmpegError as error:
            raise ValueError(
                f"{self._topic}: final video decode failed: {error}"
            ) from error
        if frames:
            raise ValueError(f"{self._topic}: decoder retained a delayed output frame")
