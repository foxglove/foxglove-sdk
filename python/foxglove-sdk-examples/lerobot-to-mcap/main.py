import argparse
import sys
import warnings
from datetime import datetime
from pathlib import Path

from foxglove.lerobot import (
    DEFAULT_EPISODE_GAP_S,
    DEFAULT_START_TIME,
    BFrameWarning,
    EpisodeWriter,
    KeyframeError,
    UnsupportedDatasetError,
    UnsupportedVideoError,
    load_metadata,
)


def parse_episodes(spec: str) -> set[int]:
    selected: set[int] = set()
    for part in map(str.strip, spec.split(",")):
        if not part:
            continue
        first, _, last = part.partition("-")
        try:
            start, end = int(first), int(last or first)
        except ValueError:
            raise argparse.ArgumentTypeError(
                f"invalid episode selection {part!r}"
            ) from None
        if end < start:
            raise argparse.ArgumentTypeError(f"invalid episode selection {part!r}")
        selected.update(range(start, end + 1))
    if not selected:
        raise argparse.ArgumentTypeError("no episodes selected")
    return selected


def parse_start_time(value: str) -> datetime:
    iso = value[:-1] + "+00:00" if value.endswith("Z") else value
    try:
        return datetime.fromisoformat(iso)
    except ValueError:
        raise argparse.ArgumentTypeError(f"invalid ISO 8601 time {value!r}") from None


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Convert a LeRobot dataset into MCAP files, one per episode."
    )
    parser.add_argument(
        "--input",
        type=Path,
        required=True,
        help="LeRobot dataset directory, the one holding meta/, data/ and videos/",
    )
    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="directory to write MCAP files to, created if missing",
    )
    parser.add_argument(
        "--episodes",
        type=parse_episodes,
        help="episodes to convert, e.g. '0,3,10-12' (default: all)",
    )
    parser.add_argument(
        "--start-time",
        type=parse_start_time,
        default=DEFAULT_START_TIME,
        help="ISO 8601 time the dataset's first episode starts at. LeRobot records "
        "no wall-clock time, so episodes are laid end to end from here "
        "(default: %(default)s)",
    )
    parser.add_argument(
        "--episode-gap",
        type=float,
        default=DEFAULT_EPISODE_GAP_S,
        help="seconds between the end of one episode and the start of the next "
        "(default: %(default)s)",
    )
    parser.add_argument(
        "--strict-keyframes",
        action="store_true",
        help="fail if an episode's video doesn't start on a keyframe, instead of "
        "including the frames back to the previous keyframe",
    )
    args = parser.parse_args()

    try:
        metadata = load_metadata(args.input.resolve())
    except UnsupportedDatasetError as err:
        sys.exit(f"error: {err}")

    if args.episodes is None:
        episodes = list(metadata.episodes)
    else:
        unknown = args.episodes - {episode.index for episode in metadata.episodes}
        if unknown:
            sys.exit(f"error: the dataset has no episode(s) {sorted(unknown)}")
        episodes = [e for e in metadata.episodes if e.index in args.episodes]

    try:
        writer = EpisodeWriter(
            metadata,
            start_time=args.start_time,
            episode_gap_s=args.episode_gap,
            strict_keyframes=args.strict_keyframes,
        )
    except ValueError as err:
        sys.exit(f"error: {err}")
    for feature, reason in writer.skipped:
        print(f"warning: skipping {feature.key}: {reason}", file=sys.stderr)

    args.output.mkdir(parents=True, exist_ok=True)
    warnings.simplefilter("ignore", BFrameWarning)
    total_bytes = 0
    warned_b_frames: set[str] = set()
    for episode in episodes:
        try:
            written = writer.write(episode, args.output)
        except (
            FileNotFoundError,
            KeyframeError,
            UnsupportedVideoError,
            ValueError,
        ) as err:
            sys.exit(f"error: episode {episode.index}: {err}")
        size = written.path.stat().st_size
        total_bytes += size
        print(
            f"episode {episode.index}: {episode.length} frames -> {written.path} "
            f"({size / 1e6:.1f} MB)"
        )
        for key, frames_early in written.preroll_frames.items():
            print(
                f"  {key} started {frames_early} frame(s) early, at the previous "
                "keyframe, so its first frame decodes"
            )
        for key in written.b_frame_videos:
            if key not in warned_b_frames:
                warned_b_frames.add(key)
                print(
                    f"warning: {key} has B-frames, which Foxglove can't play back. "
                    "Its frames are written as they are, in decode order. To view it, "
                    "re-encode the dataset's videos without B-frames, e.g. with "
                    "ffmpeg's -bf 0.",
                    file=sys.stderr,
                )
    print(f"wrote {len(episodes)} episode(s), {total_bytes / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
