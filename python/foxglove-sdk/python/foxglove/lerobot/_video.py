from collections.abc import Iterator
from dataclasses import dataclass
from fractions import Fraction
from pathlib import Path

import av
from av.bitstream import BitStreamFilterContext
from av.container import InputContainer
from av.video.stream import VideoStream

from ._dataset import VideoSegment

FORMATS = {"av1": "av1", "h264": "h264", "hevc": "h265", "vp9": "vp9"}
ANNEX_B_FILTERS = {"h264": "h264_mp4toannexb", "h265": "hevc_mp4toannexb"}
AV1_SEQUENCE_HEADER = 1
AV1C_HEADER_SIZE = 4
H264_SPS = 7
AVCC_HEADER_SIZE = 5
ANNEX_B_START_CODE = b"\x00\x00\x00\x01"


class UnsupportedVideoError(Exception):
    """A video can't be written as ``foxglove.CompressedVideo``, for example because it uses a
    codec other than AV1, H.264, H.265 or VP9.
    """


class BFrameWarning(UserWarning):
    """A video has B-frames. Its frames are written in decode order, which Foxglove can't play
    back. Re-encode it without B-frames to view it, e.g. with ffmpeg's ``-bf 0``.
    """


class KeyframeError(Exception):
    """An episode's video doesn't start on a keyframe, and either there's no earlier
    keyframe to start from or ``strict_keyframes`` is set."""


@dataclass(frozen=True)
class VideoPacket:
    format: str
    data: bytes
    offset_s: float
    decode_offset_s: float
    is_preroll: bool
    is_b_frame: bool


def read_episode_video(
    segment: VideoSegment, fps: float, *, strict_keyframes: bool = False
) -> Iterator[VideoPacket]:
    if not segment.path.exists():
        raise FileNotFoundError(f"missing video file {segment.path}")

    half_frame_s = 0.5 / fps
    with av.open(str(segment.path)) as container:
        stream = container.streams.video[0]
        video_format = _video_format(segment.path, stream)
        time_base = stream.time_base
        if time_base is None:
            raise UnsupportedVideoError(f"{segment.path.name}: stream has no time base")

        annex_b = None
        if video_format in ANNEX_B_FILTERS:
            annex_b = BitStreamFilterContext(ANNEX_B_FILTERS[video_format], stream)
        extradata = bytes(stream.codec_context.extradata or b"")
        sequence_header = b""
        if video_format == "av1":
            sequence_header = extradata[AV1C_HEADER_SIZE:]
        parameter_sets = b""
        if video_format == "h264":
            parameter_sets = _annex_b_parameter_sets(extradata)

        decode_delay: int | None = None
        if segment.start_s > 0:
            decode_delay = _stream_decode_delay(container, stream)
            _seek_to_keyframe_shown_before(
                container, stream, time_base, segment.start_s + half_frame_s
            )

        # Frames not written yet: before the episode's first frame, the ones since the
        # latest keyframe, which it may depend on. After it, the ones shown after the
        # episode since its latest frame, which a later one may depend on.
        held: list[VideoPacket] = []
        started = False
        start_keyframe_pts: int | None = None
        latest_pts: int | None = None
        for packet in container.demux(stream):
            pts, is_keyframe = packet.pts, packet.is_keyframe
            if pts is None:
                continue
            dts = pts if packet.dts is None else packet.dts
            # Every frame shown before the end is decoded before it.
            if float(dts * time_base) >= segment.end_s - half_frame_s:
                break
            if decode_delay is None:
                decode_delay = pts - dts
            is_b_frame = latest_pts is not None and pts < latest_pts
            latest_pts = pts if latest_pts is None else max(latest_pts, pts)

            data = _take_payload(packet, annex_b)
            if video_format == "av1" and is_keyframe:
                data = _with_sequence_header(segment.path, data, sequence_header)
            if video_format == "h264" and is_keyframe:
                data = _with_parameter_sets(data, parameter_sets)

            time_s = float(pts * time_base)
            # Shifted so that frames decoded in display order are logged when shown.
            decode_time_s = float((dts + decode_delay) * time_base)
            frame = VideoPacket(
                format=video_format,
                data=data,
                offset_s=time_s - segment.start_s,
                decode_offset_s=decode_time_s - segment.start_s,
                is_preroll=time_s < segment.start_s - half_frame_s,
                is_b_frame=is_b_frame,
            )
            in_episode = (
                segment.start_s - half_frame_s <= time_s < segment.end_s - half_frame_s
            )
            if not started and is_keyframe and time_s < segment.start_s + half_frame_s:
                held = []
                start_keyframe_pts = pts
            if not in_episode:
                if start_keyframe_pts is not None and pts >= start_keyframe_pts:
                    held.append(frame)
                continue
            if not started:
                started = True
                if start_keyframe_pts is None:
                    raise KeyframeError(
                        f"{segment.path.name}: no keyframe at or before "
                        f"{segment.start_s:.3f}s"
                    )
                if held and strict_keyframes:
                    raise KeyframeError(
                        f"{segment.path.name}: the episode starting at "
                        f"{segment.start_s:.3f}s doesn't start on a keyframe"
                    )
            yield from held
            held = []
            yield frame

        if not started:
            raise ValueError(
                f"{segment.path.name}: no frames between {segment.start_s:.3f}s "
                f"and {segment.end_s:.3f}s"
            )


def _seek_to_keyframe_shown_before(
    container: InputContainer, stream: VideoStream, time_base: Fraction, time_s: float
) -> None:
    target = int(time_s / time_base)
    previous_pts = None
    while True:
        container.seek(target, stream=stream, backward=True, any_frame=False)
        keyframe = next(
            (packet for packet in container.demux(stream) if packet.pts is not None),
            None,
        )
        if (
            keyframe is None
            or keyframe.pts is None
            or float(keyframe.pts * time_base) < time_s
            or keyframe.pts == previous_pts
        ):
            break
        previous_pts = keyframe.pts
        target = (keyframe.pts if keyframe.dts is None else keyframe.dts) - 1
    container.seek(target, stream=stream, backward=True, any_frame=False)


def _stream_decode_delay(container: InputContainer, stream: VideoStream) -> int:
    for packet in container.demux(stream):
        if packet.pts is not None:
            return 0 if packet.dts is None else packet.pts - packet.dts
    return 0


def _video_format(path: Path, stream: VideoStream) -> str:
    codec = stream.codec_context.codec.canonical_name
    video_format = FORMATS.get(codec)
    if video_format is None:
        raise UnsupportedVideoError(
            f"{path.name}: {codec} video has no foxglove.CompressedVideo equivalent "
            f"(supported: {', '.join(sorted(FORMATS))})"
        )
    return video_format


def _take_payload(packet: av.Packet, annex_b: BitStreamFilterContext | None) -> bytes:
    if annex_b is None:
        return bytes(packet)
    return b"".join(bytes(out) for out in annex_b.filter(packet))


def _with_sequence_header(path: Path, data: bytes, sequence_header: bytes) -> bytes:
    if AV1_SEQUENCE_HEADER in _obu_types(data):
        return data
    if not sequence_header:
        raise UnsupportedVideoError(
            f"{path.name}: a keyframe has no AV1 sequence header, and the mp4 has none"
        )
    return sequence_header + data


def _annex_b_parameter_sets(avcc: bytes) -> bytes:
    if not avcc or avcc[0] != 1:
        return avcc
    nal_units = []
    offset = AVCC_HEADER_SIZE
    for count_mask in (0x1F, 0xFF):
        count = avcc[offset] & count_mask
        offset += 1
        for _ in range(count):
            size = int.from_bytes(avcc[offset : offset + 2], "big")
            offset += 2
            nal_units.append(ANNEX_B_START_CODE + avcc[offset : offset + size])
            offset += size
    return b"".join(nal_units)


def _with_parameter_sets(data: bytes, parameter_sets: bytes) -> bytes:
    if H264_SPS in _nal_types(data):
        return data
    return parameter_sets + data


def _nal_types(data: bytes) -> set[int]:
    types = set()
    offset = data.find(b"\x00\x00\x01")
    while offset >= 0 and offset + 3 < len(data):
        types.add(data[offset + 3] & 0x1F)
        offset = data.find(b"\x00\x00\x01", offset + 3)
    return types


def _obu_types(data: bytes) -> set[int]:
    types = set()
    offset = 0
    while offset < len(data):
        header = data[offset]
        types.add((header >> 3) & 0xF)
        has_extension = header & 0x4
        has_size = header & 0x2
        offset += 2 if has_extension else 1
        if not has_size:
            break
        size = 0
        shift = 0
        while offset < len(data):
            byte = data[offset]
            offset += 1
            size |= (byte & 0x7F) << shift
            shift += 7
            if not byte & 0x80:
                break
        offset += size
    return types
