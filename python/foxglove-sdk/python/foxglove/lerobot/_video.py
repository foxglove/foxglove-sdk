from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path

import av
from av.bitstream import BitStreamFilterContext
from av.video.stream import VideoStream

from ._dataset import VideoSegment

FORMATS = {"av1": "av1", "h264": "h264", "hevc": "h265", "vp9": "vp9"}
ANNEX_B_FILTERS = {"h264": "h264_mp4toannexb", "h265": "hevc_mp4toannexb"}
AV1_SEQUENCE_HEADER = 1
AV1C_HEADER_SIZE = 4


class UnsupportedVideoError(Exception):
    """A video can't be written as ``foxglove.CompressedVideo``, for example because it uses a
    codec other than AV1, H.264, H.265 or VP9.
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
        sequence_header = b""
        if video_format == "av1":
            av1c = bytes(stream.codec_context.extradata or b"")
            sequence_header = av1c[AV1C_HEADER_SIZE:]

        if segment.start_s > 0:
            container.seek(
                int((segment.start_s + half_frame_s) / time_base),
                stream=stream,
                backward=True,
                any_frame=False,
            )

        # Frames not written yet: before the episode's first frame, the ones since the
        # latest keyframe, which it may depend on. After it, the ones shown after the
        # episode since its latest frame, which a later one may depend on.
        held: list[VideoPacket] = []
        started = False
        latest_pts: int | None = None
        decode_delay: int | None = None
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
            if not started:
                if is_keyframe:
                    held = []
                if not in_episode:
                    if is_keyframe or held:
                        held.append(frame)
                    continue
                started = True
                if not is_keyframe:
                    if not held:
                        raise KeyframeError(
                            f"{segment.path.name}: no keyframe at or before "
                            f"{segment.start_s:.3f}s"
                        )
                    if strict_keyframes:
                        raise KeyframeError(
                            f"{segment.path.name}: the episode starting at "
                            f"{segment.start_s:.3f}s doesn't start on a keyframe"
                        )
            elif not in_episode:
                held.append(frame)
                continue
            yield from held
            held = []
            yield frame

        if not started:
            raise ValueError(
                f"{segment.path.name}: no frames between {segment.start_s:.3f}s "
                f"and {segment.end_s:.3f}s"
            )


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
