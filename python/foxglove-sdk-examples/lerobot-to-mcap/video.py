from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path

import av
from av.bitstream import BitStreamFilterContext
from av.video.stream import VideoStream
from lerobot_dataset import VideoSegment

FORMATS = {"av1": "av1", "h264": "h264", "hevc": "h265", "vp9": "vp9"}
ANNEX_B_FILTERS = {"h264": "h264_mp4toannexb", "h265": "hevc_mp4toannexb"}
AV1_SEQUENCE_HEADER = 1
AV1C_HEADER_SIZE = 4


class UnsupportedVideoError(Exception):
    pass


class KeyframeError(Exception):
    pass


@dataclass(frozen=True)
class VideoPacket:
    format: str
    data: bytes
    offset_s: float
    is_preroll: bool


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

        preroll: list[VideoPacket] = []
        started = False
        last_pts: int | None = None
        for packet in container.demux(stream):
            pts, is_keyframe = packet.pts, packet.is_keyframe
            if pts is None:
                continue
            if last_pts is not None and pts < last_pts:
                raise UnsupportedVideoError(
                    f"{segment.path.name}: the video has B-frames, which Foxglove can't "
                    "play back. Re-encode it without them, e.g. with ffmpeg's `-bf 0`."
                )
            last_pts = pts

            time_s = float(pts * time_base)
            if time_s >= segment.end_s - half_frame_s:
                break

            data = _take_payload(packet, annex_b)
            if video_format == "av1" and is_keyframe:
                data = _with_sequence_header(segment.path, data, sequence_header)

            is_preroll = time_s < segment.start_s - half_frame_s
            frame = VideoPacket(
                format=video_format,
                data=data,
                offset_s=time_s - segment.start_s,
                is_preroll=is_preroll,
            )
            if is_preroll:
                if is_keyframe:
                    preroll = [frame]
                elif preroll:
                    preroll.append(frame)
                continue

            if not started:
                started = True
                if not is_keyframe:
                    if not preroll:
                        raise KeyframeError(
                            f"{segment.path.name}: no keyframe at or before "
                            f"{segment.start_s:.3f}s"
                        )
                    if strict_keyframes:
                        raise KeyframeError(
                            f"{segment.path.name}: the episode starting at "
                            f"{segment.start_s:.3f}s doesn't start on a keyframe"
                        )
                    yield from preroll
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
