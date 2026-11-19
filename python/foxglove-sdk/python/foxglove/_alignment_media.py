"""Private alignment codec implementation; no stable SDK interface is promised.

Requires the pinned PyAV/Pillow runtime. Rust owns source envelopes, identity,
resource reservations, bounded scheduling and atomic output publication.
"""

from __future__ import annotations

import copy
import hashlib
import io
import math
import re
from fractions import Fraction
from typing import Any, cast

try:
    import av
    from PIL import Image, features
except ModuleNotFoundError as exc:
    raise ImportError(
        "Alignment media requires Python 3.11 or later and foxglove-sdk[alignment]"
    ) from exc

_START_CODE = re.compile(b"\x00\x00\x00?\x01")
_MAX_INTEGER = (1 << 63) - 1
_PYAV_VERSION = "16.1.0"
_PILLOW_VERSION = "13.0.0"


class MediaError(ValueError):
    """A categorized error crossing the private native media boundary."""

    def __init__(self, kind: str, message: str) -> None:
        super().__init__(message)
        self.kind = kind


def _fail(kind: str, message: str) -> None:
    raise MediaError(kind, message)


def _check_runtime() -> None:
    if av.__version__ != _PYAV_VERSION or Image.__version__ != _PILLOW_VERSION:
        _fail(
            "unsupported_media",
            f"Alignment requires PyAV {_PYAV_VERSION} and Pillow {_PILLOW_VERSION}",
        )


def capabilities() -> dict[str, Any]:
    """Describe actual software codec availability and the strict stream subset."""
    _check_runtime()
    codecs = {}
    for name, native in (("h264", "h264"), ("h265", "hevc")):
        try:
            codec = av.Codec(native, "r")
            codecs[name] = {"available": codec.type == "video", "decoder": codec.name}
        except (ValueError, av.error.FFmpegError):
            codecs[name] = {"available": False, "decoder": native}
    return {
        "backend": "PyAV",
        "version": av.__version__,
        "ffmpeg_libraries": av.library_versions,
        "ffmpeg_version": av.ffmpeg_version_info,
        "pillow": Image.__version__,
        "codecs": codecs,
        "framing": "annex_b",
        "b_pictures": False,
        "pixel_formats": ["yuv420p"],
        "bit_depth": 8,
        "codec_threads": 1,
        "codec_thread_type": "NONE",
        "codec_worker_threads": 0,
        "pending_tokens": 128,
        "representation": "FFmpeg software yuv420p -> RGB24 via libswscale defaults",
    }


def _positive(value: Any, name: str) -> int:
    if type(value) is not int or value <= 0 or value > _MAX_INTEGER:
        _fail("invalid_configuration", f"{name} must be a positive signed-64 integer")
    return cast(int, value)


def _resize_config(resize: dict[str, Any] | None) -> dict[str, Any]:
    config = dict(resize) if resize is not None else {"kind": "none"}
    if config == {"kind": "none"}:
        return config
    if config.get("kind") != "bounded_aspect" or set(config) - {
        "kind",
        "max_width",
        "max_height",
    }:
        _fail("invalid_configuration", "Unknown resize kind or keys")
    if not any(key in config for key in ("max_width", "max_height")):
        _fail("invalid_configuration", "At least one resize bound is required")
    for key in ("max_width", "max_height"):
        if key in config:
            _positive(config[key], key)
    return config


def resolve_geometry(
    width: int, height: int, resize: dict[str, Any] | None = None
) -> dict[str, Any]:
    """Resolve rational floor aspect fit and integer-centered image geometry."""
    _positive(width, "source width")
    _positive(height, "source height")
    config = _resize_config(resize)
    scale = min(
        [Fraction(1)]
        + [
            Fraction(config[key], size)
            for key, size in (("max_width", width), ("max_height", height))
            if key in config
        ]
    )
    output = (int(width * scale), int(height * scale))
    if min(output) == 0:
        _fail("invalid_configuration", "Resize bounds yield a zero image dimension")
    sx, sy = Fraction(output[0], width), Fraction(output[1], height)
    return {
        "source_dimensions": [width, height],
        "output_dimensions": list(output),
        "sx": [sx.numerator, sx.denominator],
        "sy": [sy.numerator, sy.denominator],
        "A": [
            [float(sx), 0.0, float((sx - 1) / 2)],
            [0.0, float(sy), float((sy - 1) / 2)],
            [0.0, 0.0, 1.0],
        ],
        "resize": config,
        "resampler": {
            "backend": "Pillow",
            "version": Image.__version__,
            "filter": "LANCZOS",
            "radius": 3,
            "box": None,
            "reducing_gap": None,
            "bypassed": output == (width, height),
        },
    }


def _matrix(value: Any, length: int, name: str) -> list[float]:
    if not isinstance(value, (list, tuple)) or len(value) != length:
        _fail("unsupported_media", f"Calibration {name} must contain {length} values")
    if any(type(v) not in (int, float) or not math.isfinite(v) for v in value):
        _fail("unsupported_media", f"Calibration {name} must contain finite numbers")
    return [float(v) for v in value]


def derive_calibration(
    calibration: dict[str, Any], geometry: dict[str, Any]
) -> dict[str, Any]:
    """Transform validated normalized pinhole calibration with the full A product.

    Native orchestration validates binding identity, temporal constancy and schema.
    Unknown fields such as extrinsics are retained without modification.
    """
    if calibration.get("pixel_coordinates") != "integer_centers":
        _fail("unsupported_media", "Calibration requires integer_centers declaration")
    if [calibration.get("width"), calibration.get("height")] != geometry[
        "source_dimensions"
    ] or any(type(calibration.get(k)) is not int for k in ("width", "height")):
        _fail("unsupported_media", "Calibration dimensions differ from source image")
    model = calibration.get("distortion_model")
    if model not in ("plumb_bob", "rational_polynomial"):
        _fail("unsupported_media", "Unsupported calibration distortion model")
    _matrix(calibration.get("D"), 5 if model == "plumb_bob" else 8, "D")
    intrinsic = _matrix(calibration.get("K"), 9, "K")
    if intrinsic[0] <= 0 or intrinsic[4] <= 0 or intrinsic[6:] != [0, 0, 1]:
        _fail("unsupported_media", "Uncalibrated or noncanonical intrinsic matrix")
    result = copy.deepcopy(calibration)
    a = geometry["A"]

    def product(matrix: list[float], columns: int) -> list[float]:
        transformed = [
            sum(a[row][inner] * matrix[inner * columns + column] for inner in range(3))
            for row in range(3)
            for column in range(columns)
        ]
        if not all(math.isfinite(v) for v in transformed):
            _fail("unsupported_media", "Transformed calibration is not finite")
        return transformed

    result["K"] = product(intrinsic, 3)
    if "R" in calibration:
        _matrix(calibration["R"], 9, "R")
    if "P" in calibration:
        projection = _matrix(calibration["P"], 12, "P")
        if projection[8:] != [0, 0, 1, 0]:
            _fail("unsupported_media", "Noncanonical projection matrix")
        result["P"] = product(projection, 4)
    result["width"], result["height"] = geometry["output_dimensions"]
    return result


def _encoding(config: dict[str, Any] | None) -> dict[str, Any]:
    value = dict(config) if config is not None else {"kind": "png"}
    if value == {"kind": "png"}:
        return {"kind": "png", "compress_level": 1, "optimize": False}
    if set(value) == {"kind", "quality"} and value["kind"] == "jpeg":
        quality = value["quality"]
        if type(quality) is int and 1 <= quality <= 100:
            return {
                "kind": "jpeg",
                "quality": quality,
                "subsampling": 0,
                "optimize": False,
                "progressive": False,
            }
    _fail("invalid_configuration", "Expected PNG or JPEG with explicit quality 1..100")
    raise AssertionError("unreachable")


class _BoundedBytes(io.BytesIO):
    def __init__(self, limit: int) -> None:
        super().__init__()
        self.limit = limit

    def write(self, value: Any) -> int:
        if self.tell() + len(value) > self.limit:
            _fail("resource_limit", "Encoded image exceeds frame buffer capacity")
        return super().write(value)


def _encode_rgb(
    image: Image.Image,
    geometry: dict[str, Any],
    encoding: dict[str, Any],
    encoded_capacity: int,
) -> tuple[bytes, str]:
    output_size = tuple(geometry["output_dimensions"])
    if image.size != output_size:
        image = image.resize(
            output_size, resample=Image.Resampling.LANCZOS, box=None, reducing_gap=None
        )
    output = _BoundedBytes(encoded_capacity)
    options = {key: value for key, value in encoding.items() if key != "kind"}
    image.save(output, format=encoding["kind"].upper(), **options)
    return output.getvalue(), hashlib.sha256(image.tobytes()).hexdigest()


def _check_png_rgb8(data: bytes) -> None:
    if not data.startswith(b"\x89PNG\r\n\x1a\n"):
        return
    if len(data) < 33 or data[8:16] != b"\x00\x00\x00\rIHDR":
        _fail("decode_failed", "PNG requires a complete canonical IHDR")
    # Pillow exposes 16-bit truecolor as mode RGB after discarding low bits.
    # Inspect the original header before any pixel decoder can normalize depth.
    if data[24] != 8 or data[25] != 2:
        _fail("unsupported_media", "Standalone PNG requires RGB8 truecolor")


def process_image(
    data: bytes,
    format: str,
    resize: dict[str, Any] | None = None,
    encoding: dict[str, Any] | None = None,
    max_frame_bytes: int = 64 * 1024 * 1024,
) -> tuple[bytes, dict[str, Any]]:
    """Materialize a canonical standalone PNG/JPEG RGB8 image.

    Envelope/source identity and timing stay with native orchestration. ICC and
    EXIF transforms are not applied; the declared pixels are decoded RGB values.
    The caller caches the result by source identity, transform and encoding.
    """
    capabilities()
    capacity = _positive(max_frame_bytes, "max_frame_bytes")
    requested_resize = _resize_config(resize)
    requested_encoding = _encoding(encoding)
    if format not in ("png", "jpeg"):
        _fail("unsupported_media", f"Unsupported standalone image format {format}")
    if not isinstance(data, bytes) or len(data) > capacity:
        _fail("resource_limit", "Image packet exceeds frame buffer capacity")
    if format == "png":
        _check_png_rgb8(data)
    try:
        with Image.open(io.BytesIO(data), formats=(format.upper(),)) as image:
            if (
                image.mode != "RGB"
                or getattr(image, "n_frames", 1) != 1
                or "transparency" in image.info
            ):
                _fail(
                    "unsupported_media", "Standalone images require single-frame RGB8"
                )
            pixels = image.width * image.height
            if pixels * 16 > capacity:
                _fail("resource_limit", "Decoded image exceeds reserved media buffers")
            geometry = resolve_geometry(image.width, image.height, requested_resize)
            properties = {
                "width": image.width,
                "height": image.height,
                "pixel_format": "rgb24",
                "input_format": format,
            }
            image.load()
            try:
                encoded, pixel_hash = _encode_rgb(
                    image,
                    geometry,
                    requested_encoding,
                    capacity - pixels * 12,
                )
            except MediaError:
                raise
            except (ValueError, OSError) as exc:
                raise MediaError("encode_failed", str(exc)) from exc
    except MediaError:
        raise
    except Image.DecompressionBombError as exc:
        raise MediaError("resource_limit", str(exc)) from exc
    except (ValueError, OSError) as exc:
        raise MediaError("decode_failed", str(exc)) from exc
    return encoded, {
        "geometry": geometry,
        "encoding": requested_encoding,
        "decoded_properties": properties,
        "backend": {
            "name": "Pillow",
            "version": Image.__version__,
            "decoder": format,
            "codec_threads": 1,
            "jpeg_library": features.version_codec("jpg"),
            "libjpeg_turbo": features.version_feature("libjpeg_turbo"),
            "zlib_library": features.version_codec("zlib"),
        },
        "pixel_representation": "Pillow decoded RGB8; no ICC or EXIF transform",
        "pixel_sha256": pixel_hash,
    }


class Decoder:
    """One isolated non-B Annex B stream, with bounded pending token association.

    Each input envelope contains one frame. Tokens are private monotonic ordinals;
    they carry no presentation-time claim. Decoded frames are validated, but only
    selected frames become RGB/images. Caller must drain each returned list before
    feeding more packets. A decoder belongs to exactly one source file/channel.
    """

    def __init__(
        self,
        codec: str,
        resize: dict[str, Any] | None = None,
        encoding: dict[str, Any] | None = None,
        max_frame_bytes: int = 64 * 1024 * 1024,
        codec_threads: int = 1,
    ) -> None:
        _check_runtime()
        if codec not in ("h264", "h265"):
            _fail("unsupported_media", f"Unsupported codec {codec}")
        # Independent camera workers provide concurrency; a codec never adds threads.
        if type(codec_threads) is not int or codec_threads != 1:
            _fail("invalid_configuration", "Alignment codecs require one thread")
        self.codec = codec
        self.resize = _resize_config(resize)
        self.encoding = _encoding(encoding)
        self.max_frame_bytes = _positive(max_frame_bytes, "max_frame_bytes")
        try:
            name = "hevc" if codec == "h265" else codec
            available = av.Codec(name, "r")
            if available.type != "video":
                _fail("unsupported_media", f"No software video decoder for {codec}")
            self.context = cast(av.VideoCodecContext, av.CodecContext.create(name, "r"))
            self.context.thread_count = 1
            self.context.thread_type = "NONE"
            self.context.options = {
                "err_detect": "explode",
                "max_pixels": str(self.max_frame_bytes // 16),
            }
        except (ValueError, av.error.FFmpegError) as exc:
            raise MediaError(
                "unsupported_media", f"Decoder unavailable: {codec}: {exc}"
            ) from exc
        self.pending: dict[int, bool] = {}
        self.parameters: dict[int, bytes] = {}
        self.properties: dict[str, Any] | None = None
        self.geometry: dict[str, Any] | None = None
        self.packet_count = 0
        self.frame_count = 0
        self.encode_count = 0
        self.flush_frames = 0
        self.last_token = -1
        self.finished = False
        self.closed = False

    def _check_open(self) -> None:
        if self.closed or self.finished:
            _fail("decode_failed", "Decoder already flushed or closed")

    def _validate_packet(self, data: bytes) -> bool:
        """Validate the Annex B envelope and report whether decoding can proceed."""
        if not isinstance(data, bytes) or len(data) > self.max_frame_bytes:
            _fail("resource_limit", "Packet exceeds frame buffer capacity")
        starts: list[re.Match[bytes]] = []
        for match in _START_CODE.finditer(data):
            if len(starts) >= 4096:
                _fail("resource_limit", "Packet exceeds 4096 NAL units")
            starts.append(match)
        if not starts or starts[0].start() != 0:
            _fail("unsupported_media", "Expected Annex B packet framing")
        kinds = set()
        parameters = {7, 8} if self.codec == "h264" else {32, 33, 34}
        for index, start in enumerate(starts):
            end = starts[index + 1].start() if index + 1 < len(starts) else len(data)
            unit = data[start.end() : end]
            if len(unit) < (1 if self.codec == "h264" else 2) or unit[0] & 0x80:
                _fail("decode_failed", "Malformed Annex B NAL header")
            if self.codec == "h265" and unit[1] & 7 == 0:
                _fail("decode_failed", "Invalid HEVC temporal_id_plus1")
            kind = unit[0] & 31 if self.codec == "h264" else (unit[0] >> 1) & 63
            kinds.add(kind)
            if kind in parameters:
                digest = hashlib.sha256(unit).digest()
                if kind in self.parameters and self.parameters[kind] != digest:
                    _fail("unsupported_media", f"Changed parameter set {kind}")
                self.parameters[kind] = digest
        pictures = {1, 5} if self.codec == "h264" else set(range(32))
        if not kinds.intersection(pictures):
            _fail("decode_failed", "Envelope contains no picture NAL")
        key = (
            5 in kinds
            if self.codec == "h264"
            else bool(kinds.intersection(range(16, 24)))
        )
        if self.packet_count == 0 and (not key or not parameters.issubset(kinds)):
            self.parameters.clear()
            return False
        required_key_parameters = {7} if self.codec == "h264" else parameters
        if key and not required_key_parameters.issubset(kinds):
            _fail("unsupported_media", "Keyframe missing required parameter sets")
        return True

    def feed(
        self, data: bytes, token: int, selected: bool
    ) -> list[tuple[int, bytes, dict[str, Any]]] | None:
        self._check_open()
        if type(token) is not int or not self.last_token < token <= _MAX_INTEGER:
            _fail(
                "decode_failed", "Packet tokens must be increasing nonnegative ordinals"
            )
        if type(selected) is not bool:
            _fail("invalid_configuration", "selected must be boolean")
        if not self._validate_packet(data):
            return None
        if len(self.pending) >= 128:
            _fail("resource_limit", "Decoder association backlog exceeds 128")
        packet = av.Packet(data)
        packet.pts = token
        packet.time_base = Fraction(1, 1_000_000_000)
        self.pending[token] = selected
        self.last_token = token
        self.packet_count += 1
        try:
            return self._consume(self.context.decode(packet))
        except av.error.FFmpegError as exc:
            raise MediaError("decode_failed", str(exc)) from exc

    def _consume(self, frames: list[Any]) -> list[tuple[int, bytes, dict[str, Any]]]:
        results = []
        encoded_bytes = 0
        for frame in frames:
            token = frame.pts
            if token not in self.pending:
                _fail("decode_failed", "Missing or duplicate frame token")
            selected = self.pending.pop(token)
            if frame.is_corrupt:
                _fail("decode_failed", "Decoder reported corrupt frame")
            if frame.pict_type == av.video.frame.PictureType.B:
                _fail(
                    "unsupported_media",
                    "B pictures require explicit decode/presentation order",
                )
            if frame.format.name != "yuv420p":
                _fail(
                    "unsupported_media", f"Unsupported pixel format {frame.format.name}"
                )
            props = {
                "width": frame.width,
                "height": frame.height,
                "pixel_format": frame.format.name,
                "color_range": int(frame.color_range),
                "colorspace": int(frame.colorspace),
                "profile": self.context.profile,
            }
            if self.properties is not None and props != self.properties:
                _fail("unsupported_media", "Midstream decoded configuration change")
            if self.properties is None:
                self.properties = props
                self.geometry = resolve_geometry(frame.width, frame.height, self.resize)
            # RGB, Pillow and encoded return buffers coexist within this reservation.
            if frame.width * frame.height * 16 > self.max_frame_bytes:
                _fail("resource_limit", "Decoded frame exceeds reserved media buffers")
            self.frame_count += 1
            if not selected:
                continue
            assert self.geometry is not None
            try:
                image = frame.to_image()
                encoded, pixel_hash = _encode_rgb(
                    image,
                    self.geometry,
                    self.encoding,
                    self.max_frame_bytes
                    - encoded_bytes
                    - frame.width * frame.height * 12,
                )
            except MediaError:
                raise
            except (ValueError, OSError, av.error.FFmpegError) as exc:
                raise MediaError("encode_failed", str(exc)) from exc
            encoded_bytes += len(encoded)
            metadata = {
                "geometry": self.geometry,
                "encoding": self.encoding,
                "decoded_properties": self.properties,
                "backend": {
                    "name": "PyAV",
                    "version": av.__version__,
                    "ffmpeg_libraries": av.library_versions,
                    "ffmpeg_version": av.ffmpeg_version_info,
                    "decoder": self.context.codec.name,
                    "codec_threads": 1,
                    "pillow": Image.__version__,
                },
                "pixel_representation": (
                    "FFmpeg software yuv420p -> RGB24 via libswscale defaults"
                ),
                "pixel_sha256": pixel_hash,
            }
            results.append((int(token), encoded, metadata))
            self.encode_count += 1
        return results

    def flush(self) -> list[tuple[int, bytes, dict[str, Any]]]:
        self._check_open()
        self.finished = True
        try:
            frames = self.context.decode(None)
            self.flush_frames = len(frames)
            result = self._consume(frames)
        except av.error.FFmpegError as exc:
            raise MediaError("decode_failed", str(exc)) from exc
        if self.pending or self.frame_count != self.packet_count:
            _fail(
                "decode_failed",
                "Expected exactly one frame per source envelope after flush",
            )
        return result

    def close(self) -> None:
        """Release decoder surfaces and associations without flushing failed input."""
        self.pending.clear()
        self.context = None  # type: ignore[assignment]
        self.closed = True
