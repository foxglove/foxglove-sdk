"""Create a runnable multi-camera + telemetry job using bounded public inspection.

uv run python configure.py /absolute/input.mcap /absolute/output.mcap job.json
The script prints measured choices. Review tolerances and episode bounds for the
intended task before conversion; observed spacing does not prove clock agreement.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

from foxglove.experimental.mcap_alignment import inspect, validate_config


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("config", type=Path)
    parser.add_argument(
        "--reference", action="store_true", help="Align to latest-starting camera"
    )
    parser.add_argument(
        "--jpeg-quality", type=int, help="Opt into lossy JPEG with explicit quality"
    )
    parser.add_argument(
        "--max-width", type=int, help="Opt into aspect-preserving resize"
    )
    parser.add_argument("--max-height", type=int)
    parser.add_argument("--temporary-directory", type=Path, default=Path("/tmp"))
    parser.add_argument(
        "--telemetry",
        action="append",
        default=[],
        help="Declare telemetry messages independently copyable (repeatable)",
    )
    args = parser.parse_args()
    summary = inspect(
        {
            "paths": [str(args.source.resolve())],
            "temporary_directory": str(args.temporary_directory.resolve()),
        }
    )
    channels = summary["files"][0]["channels"]
    cameras = [c for c in channels if c["schema"] == "foxglove.CompressedVideo"]
    if not cameras or any(c["messages"] < 2 for c in cameras):
        parser.error("requires nonempty foxglove.CompressedVideo channels")
    sampled = list(cameras)
    for topic in args.telemetry:
        matches = [c for c in channels if c["topic"] == topic]
        if len(matches) != 1 or not matches[0]["messages"]:
            parser.error(f"expected one nonempty telemetry channel for {topic}")
        sampled.append(matches[0])
    origin = max(int(c["first_log_time"]) for c in sampled)
    start = min(int(c["first_log_time"]) for c in sampled)
    end = min(int(c["last_log_time"]) for c in sampled) + 1
    if origin >= end:
        parser.error("requested streams have no common time window")
    if not args.reference and origin - start >= 100_000_000:
        parser.error(
            "stream start spread exceeds grid period; choose explicit interval/origin"
        )
    reference = max(cameras, key=lambda c: int(c["first_log_time"]))
    if args.reference and int(reference["first_log_time"]) < origin:
        parser.error(
            "telemetry starts after the reference camera; choose an explicit interval"
        )
    config: dict[str, Any] = {
        "paths": [str(args.source.resolve())],
        "destination": str(args.destination.resolve()),
        "temporary_directory": str(args.temporary_directory.resolve()),
        "episode": {"start": start, "end": end},
        "timeline": (
            {
                "kind": "reference",
                "bindings": [{"file": 0, "channel": reference["channel"]}],
            }
            if args.reference
            else {"kind": "fixed", "numerator": 10, "origin": origin}
        ),
        "streams": [],
    }
    for camera in cameras:
        topic = camera["topic"]
        calibrations = [
            c
            for c in channels
            if c["schema"] == "foxglove.CameraCalibration"
            and (c["topic"].startswith(topic + "/") or c["topic"] == topic + "-info")
        ]
        if len(calibrations) > 1:
            parser.error(f"ambiguous calibration for {topic}; use explicit bindings")
        calibration: dict[str, Any] = (
            {"status": "unavailable"}
            if not calibrations
            else {
                "bindings": [{"file": 0, "channel": calibrations[0]["channel"]}],
                "pixel_coordinates": "integer_centers",
                "original_topic": topic + "/original_calibration",
                "derived_topic": topic + "/derived_calibration",
            }
        )
        media: dict[str, Any] = {"calibration": calibration}
        if args.jpeg_quality is not None:
            media["encoding"] = {"kind": "jpeg", "quality": args.jpeg_quality}
        if args.max_width is not None or args.max_height is not None:
            media["resize"] = {"kind": "bounded_aspect"}
            for key in ("max_width", "max_height"):
                if getattr(args, key) is not None:
                    media["resize"][key] = getattr(args, key)
        config["streams"].append(
            {
                "bindings": [{"file": 0, "channel": camera["channel"]}],
                "output_topic": topic + "/aligned",
                "policy": "previous",
                "required": True,
                "tolerance_ns": 1_000_000_000,
                "media": media,
            }
        )
    for topic in args.telemetry:
        config["streams"].append(
            {
                "topic": topic,
                "output_topic": topic + "/aligned",
                "policy": "previous",
                "required": True,
                "tolerance_ns": 1_000_000_000,
                "independence": "declared",
            }
        )
    resolved = validate_config(config)
    try:
        with args.config.open("x", encoding="utf-8") as config_file:
            config_file.write(json.dumps(resolved, indent=2) + "\n")
    except FileExistsError:
        parser.error(f"config path already exists: {args.config}")
    print(
        json.dumps(
            {
                "job": str(args.config),
                "inspection": summary,
                "review": (
                    "Explicit 1 s tolerance, payload independence and "
                    "integer-centered calibration convention require task review."
                ),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
