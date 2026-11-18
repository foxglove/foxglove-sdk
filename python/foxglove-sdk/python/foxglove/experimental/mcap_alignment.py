"""Experimental local MCAP alignment; see tools/mcap_alignment/README.md.

``run(config)`` performs one whole native job with bounded selection batches.
``inspect(config)`` scans one indexed recording and summarizes channel spacing
using a bounded in-memory message index. No training package
is imported. JSON configuration is strict: unknown fields and implicit integer
conversions are rejected. Nanoseconds are unsigned 64-bit integers.
"""

from __future__ import annotations

import argparse
import copy
import importlib
import json
import sys
from pathlib import Path
from typing import Any

_U64 = (1 << 64) - 1
_LIMITS = {
    "index_memory_bytes",
    "inventory_memory_bytes",
    "max_record_bytes",
    "max_chunk_bytes",
    "max_decompressed_chunk_bytes",
    "temporary_bytes",
    "max_open_files",
    "batch_targets",
    "compute_threads",
    "cache_bytes",
    "max_output_bytes",
    "writer_index_memory_bytes",
}


class AlignmentError(ValueError):
    """Categorized configuration or native job failure, including target context."""

    def __init__(
        self,
        message: str,
        *,
        kind: str = "InvalidConfiguration",
        target: Any = None,
        stream: Any = None,
        detail: Any = None,
    ) -> None:
        super().__init__(message)
        self.kind, self.target, self.stream, self.detail = kind, target, stream, detail

    def as_dict(self) -> dict[str, Any]:
        return {
            "kind": self.kind,
            "message": str(self),
            "target": self.target,
            "stream": self.stream,
            "detail": self.detail,
        }


def _object(
    value: Any, allowed: set[str], required: set[str], name: str
) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) - allowed or required - set(value):
        raise AlignmentError(
            f"{name}: expected keys {sorted(allowed)}; required {sorted(required)}"
        )
    return value


def _integer(value: Any, name: str, minimum: int = 0, maximum: int = _U64) -> None:
    if type(value) is not int or not minimum <= value <= maximum:
        raise AlignmentError(f"{name}: expected integer in [{minimum}, {maximum}]")


def _text(value: Any, name: str) -> None:
    if not isinstance(value, str) or not value or "\0" in value:
        raise AlignmentError(f"{name}: expected nonempty string without NUL")


def _choice(value: Any, choices: tuple[str, ...], name: str) -> None:
    if value not in choices:
        raise AlignmentError(f"{name}: expected one of {choices}")


def _binding(value: dict[str, Any], name: str) -> None:
    if ("topic" in value) == ("bindings" in value):
        raise AlignmentError(f"{name}: specify exactly one of topic or bindings")
    if "topic" in value:
        _text(value["topic"], name + ".topic")
    else:
        if not isinstance(value["bindings"], list) or not value["bindings"]:
            raise AlignmentError(f"{name}.bindings: expected nonempty array")
        seen = set()
        for binding in value["bindings"]:
            _object(binding, {"file", "channel"}, {"file", "channel"}, name)
            _integer(binding["file"], "file")
            _integer(binding["channel"], "channel", maximum=65535)
            pair = (binding["file"], binding["channel"])
            if pair in seen:
                raise AlignmentError(f"{name}: duplicate binding")
            seen.add(pair)


def validate_config(
    config: dict[str, Any],
    *,
    base_directory: str | Path | None = None,
    inspection: bool = False,
) -> dict[str, Any]:
    """Copy, validate and resolve paths relative to base_directory (default cwd).

    Inspection accepts only paths, temporary_directory, limits and index_mode.
    Conversion additionally requires episode, timeline, streams and destination.
    Explicit finite tolerance_ns is required for every stream, including exact.
    """
    result = copy.deepcopy(config)
    common = {"paths", "temporary_directory", "limits", "index_mode"}
    required = (
        {"paths"}
        if inspection
        else {"paths", "destination", "episode", "timeline", "streams"}
    )
    _object(
        result,
        common if inspection else common | required | {"context", "output"},
        required,
        "config",
    )
    base = Path(base_directory or Path.cwd()).resolve()
    if not isinstance(result["paths"], list) or len(result["paths"]) != 1:
        raise AlignmentError("paths: expected exactly one local indexed MCAP")
    for key in ("paths", "temporary_directory", "destination"):
        if key not in result:
            continue
        values = result[key] if key == "paths" else [result[key]]
        for value in values:
            _text(value, key)
        resolved = [str((base / value).resolve()) for value in values]
        result[key] = resolved if key == "paths" else resolved[0]
    result.setdefault("temporary_directory", str(base))
    _choice(result.get("index_mode", "indexed"), ("indexed",), "index_mode")
    limits = _object(result.get("limits", {}), _LIMITS, set(), "limits")
    for key, value in limits.items():
        _integer(value, key, 0 if key == "temporary_bytes" else 1)
    if inspection:
        return result
    if "compute_threads" in limits:
        _integer(limits["compute_threads"], "compute_threads", 1, 8)
    output = _object(
        result.get("output", {}),
        {"compression", "chunk_bytes", "zstd_level"},
        set(),
        "output",
    )
    compression = output.get("compression", "none")
    _choice(compression, ("none", "lz4", "zstd"), "output.compression")
    if "chunk_bytes" in output:
        _integer(output["chunk_bytes"], "output.chunk_bytes", 1)
        if output["chunk_bytes"] not in (262144, 1048576, 8388608):
            raise AlignmentError("output.chunk_bytes: expected 256 KiB, 1 MiB or 8 MiB")
    if "zstd_level" in output:
        _integer(output["zstd_level"], "output.zstd_level", 1, 3)
        if compression != "zstd":
            raise AlignmentError("output.zstd_level requires zstd compression")
    episode = _object(result["episode"], {"start", "end"}, {"start", "end"}, "episode")
    for key in episode:
        _integer(episode[key], key)
    if episode["start"] >= episode["end"]:
        raise AlignmentError("episode: start must precede end (half-open interval)")
    timeline = _object(
        result["timeline"],
        {"kind", "numerator", "denominator", "origin", "topic", "bindings"},
        {"kind"},
        "timeline",
    )
    _choice(timeline["kind"], ("fixed", "reference"), "timeline.kind")
    if timeline["kind"] == "fixed":
        _object(
            timeline,
            {"kind", "numerator", "denominator", "origin"},
            {"kind", "numerator"},
            "timeline",
        )
        _integer(timeline["numerator"], "numerator", 1)
        _integer(timeline.get("denominator", 1), "denominator", 1)
        _integer(timeline.get("origin", episode["start"]), "origin")
    else:
        _object(timeline, {"kind", "topic", "bindings"}, {"kind"}, "timeline")
        _binding(timeline, "timeline")
    if not isinstance(result["streams"], list) or not result["streams"]:
        raise AlignmentError("streams: expected nonempty array")
    topics = set()
    for stream in result["streams"]:
        _object(
            stream,
            {
                "topic",
                "bindings",
                "output_topic",
                "policy",
                "tolerance_ns",
                "required",
                "independence",
                "media",
            },
            {"output_topic", "policy", "tolerance_ns", "required"},
            "stream",
        )
        _binding(stream, "stream")
        _text(stream["output_topic"], "output_topic")
        if stream["output_topic"] in topics:
            raise AlignmentError("duplicate output_topic")
        topics.add(stream["output_topic"])
        _choice(stream["policy"], ("previous", "nearest", "exact"), "policy")
        _integer(stream["tolerance_ns"], "tolerance_ns")
        if stream["policy"] == "exact" and stream["tolerance_ns"] != 0:
            raise AlignmentError("exact selection requires zero tolerance")
        if type(stream.get("required", True)) is not bool:
            raise AlignmentError("required: expected boolean")
        if "media" not in stream:
            if "independence" not in stream:
                raise AlignmentError(
                    "copied streams require an explicit independence declaration"
                )
            _choice(
                stream["independence"],
                ("declared", "numeric", "standalone_image"),
                "independence",
            )
        elif "independence" in stream:
            raise AlignmentError("media and independence are mutually exclusive")
        else:
            media = _object(
                stream["media"],
                {"calibration", "resize", "encoding", "max_frame_bytes"},
                {"calibration"},
                "media",
            )
            _integer(media.get("max_frame_bytes", 64 * 1024**2), "max_frame_bytes", 1)
            resize = _object(
                media.get("resize", {"kind": "none"}),
                {"kind", "max_width", "max_height"},
                {"kind"},
                "resize",
            )
            _choice(resize["kind"], ("none", "bounded_aspect"), "resize.kind")
            if resize["kind"] == "none" and len(resize) != 1:
                raise AlignmentError("resize none takes no bounds")
            if resize["kind"] == "bounded_aspect" and len(resize) == 1:
                raise AlignmentError("bounded_aspect requires at least one bound")
            for key in ("max_width", "max_height"):
                if key in resize:
                    _integer(resize[key], key, 1, (1 << 63) - 1)
            encoding = _object(
                media.get("encoding", {"kind": "png"}),
                {"kind", "quality"},
                {"kind"},
                "encoding",
            )
            _choice(encoding["kind"], ("png", "jpeg"), "encoding.kind")
            if encoding["kind"] == "jpeg":
                if "quality" not in encoding:
                    raise AlignmentError("JPEG requires explicit quality")
                _integer(encoding["quality"], "quality", 1, 100)
            elif "quality" in encoding:
                raise AlignmentError("PNG takes no quality")
            calibration = _object(
                media["calibration"],
                {
                    "status",
                    "topic",
                    "bindings",
                    "pixel_coordinates",
                    "original_topic",
                    "derived_topic",
                },
                set(),
                "calibration",
            )
            if calibration.get("status") == "unavailable":
                _object(calibration, {"status"}, {"status"}, "calibration")
            else:
                _object(
                    calibration,
                    {
                        "topic",
                        "bindings",
                        "pixel_coordinates",
                        "original_topic",
                        "derived_topic",
                    },
                    {"pixel_coordinates", "original_topic", "derived_topic"},
                    "calibration",
                )
                _binding(calibration, "calibration")
                _choice(
                    calibration["pixel_coordinates"],
                    ("integer_centers",),
                    "pixel_coordinates",
                )
                for key in ("original_topic", "derived_topic"):
                    _text(calibration[key], key)
    contexts = result.get("context", [])
    if not isinstance(contexts, list):
        raise AlignmentError("context: expected array")
    for context in contexts:
        _object(
            context,
            {"kind", "file", "channel", "ordinal", "output_topic", "offset", "rename"},
            {"kind", "file"},
            "context",
        )
        _choice(context["kind"], ("message", "record"), "context.kind")
        _integer(context["file"], "context.file")
        if context["kind"] == "message":
            _object(
                context,
                {"kind", "file", "channel", "ordinal", "output_topic"},
                {"kind", "file", "channel", "ordinal"},
                "context",
            )
            _integer(context["channel"], "context.channel", maximum=65535)
            _integer(context["ordinal"], "context.ordinal")
            if "output_topic" in context:
                _text(context["output_topic"], "context.output_topic")
        else:
            _object(
                context,
                {"kind", "file", "offset", "rename"},
                {"kind", "file", "offset"},
                "context",
            )
            _integer(context["offset"], "context.offset")
            if "rename" in context:
                _text(context["rename"], "context.rename")
    return result


def load_config(path: str | Path) -> dict[str, Any]:
    """Load JSON without allowing duplicate keys to overwrite selection settings."""

    def unique(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                raise AlignmentError(f"duplicate JSON key: {key}")
            result[key] = value
        return result

    value = json.loads(Path(path).read_text(), object_pairs_hook=unique)
    if not isinstance(value, dict):
        raise AlignmentError("config: expected JSON object")
    return value


def _native(config: dict[str, Any]) -> dict[str, Any]:
    try:
        native = importlib.import_module("foxglove._foxglove_py")
        value: dict[str, Any] = json.loads(
            native._alignment_execute(json.dumps(config))
        )
        return value
    except ValueError as error:
        raise AlignmentError(
            str(error),
            kind=getattr(error, "kind", "InvalidConfiguration"),
            target=getattr(error, "target", None),
            stream=getattr(error, "stream", None),
            detail=getattr(error, "detail", None),
        ) from error


def run(
    config: dict[str, Any], *, base_directory: str | Path | None = None
) -> dict[str, Any]:
    """Execute one local job and return its report plus final observations.

    Existing destinations are preserved. Failed jobs do not publish an MCAP.
    Per-sample provenance streams into the output rather than this return value.
    """
    return _native(validate_config(config, base_directory=base_directory))


def inspect(
    config: dict[str, Any], *, base_directory: str | Path | None = None
) -> dict[str, Any]:
    """Validate local sources and return per-channel bounded spacing summaries."""
    resolved = validate_config(config, base_directory=base_directory, inspection=True)
    resolved["operation"] = "inspect"
    return _native(resolved)


def main() -> None:
    """Command-line entry point for inspect/run and a JSON configuration file."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("inspect", "run"))
    parser.add_argument("config", type=Path)
    args = parser.parse_args()
    try:
        config = load_config(args.config)
        function = inspect if args.operation == "inspect" else run
        print(
            json.dumps(
                function(config, base_directory=args.config.resolve().parent), indent=2
            )
        )
    except (AlignmentError, OSError, json.JSONDecodeError) as error:
        detail = (
            error.as_dict()
            if isinstance(error, AlignmentError)
            else {"kind": type(error).__name__, "message": str(error)}
        )
        print(json.dumps({"status": "failed", "error": detail}), file=sys.stderr)
        raise SystemExit(1) from error


if __name__ == "__main__":
    main()
