import argparse
import math
import sys
from datetime import datetime, timezone
from pathlib import Path

from convert import DEFAULT_EPISODE_GAP_S, DEFAULT_START_TIME, EpisodeWriter
from lerobot_dataset import UnsupportedDatasetError, load_dataset
from video import KeyframeError, UnsupportedVideoError

EARLIEST_START_TIME = datetime(1970, 1, 1, tzinfo=timezone.utc)
LATEST_START_TIME = datetime(2100, 1, 1, tzinfo=timezone.utc)


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
        start_time = datetime.fromisoformat(iso)
    except ValueError:
        raise argparse.ArgumentTypeError(f"invalid ISO 8601 time {value!r}") from None
    if start_time.tzinfo is None:
        start_time = start_time.replace(tzinfo=timezone.utc)
    if not EARLIEST_START_TIME <= start_time < LATEST_START_TIME:
        raise argparse.ArgumentTypeError(
            f"{value!r} must be at or after {EARLIEST_START_TIME:%Y-%m-%dT%H:%M:%SZ} "
            f"and before {LATEST_START_TIME:%Y-%m-%dT%H:%M:%SZ}"
        )
    return start_time


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
    if not math.isfinite(args.episode_gap) or args.episode_gap < 0:
        parser.error("--episode-gap must be a finite number of seconds, 0 or more")

    try:
        dataset = load_dataset(args.input.resolve())
    except UnsupportedDatasetError as err:
        sys.exit(f"error: {err}")

    if args.episodes is None:
        episodes = list(dataset.episodes)
    else:
        unknown = args.episodes - {episode.index for episode in dataset.episodes}
        if unknown:
            sys.exit(f"error: the dataset has no episode(s) {sorted(unknown)}")
        episodes = [e for e in dataset.episodes if e.index in args.episodes]

    try:
        writer = EpisodeWriter(
            dataset,
            start_time=args.start_time,
            episode_gap_s=args.episode_gap,
            strict_keyframes=args.strict_keyframes,
        )
    except ValueError as err:
        sys.exit(f"error: {err}")
    for feature, reason in writer.skipped:
        print(f"warning: skipping {feature.key}: {reason}", file=sys.stderr)

    args.output.mkdir(parents=True, exist_ok=True)
    total_bytes = 0
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
        if written.preroll_packets:
            print(
                f"  started {written.preroll_packets} video frame(s) early, at the "
                "previous keyframe, so the episode's first frame decodes"
            )
    print(f"wrote {len(episodes)} episode(s), {total_bytes / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
