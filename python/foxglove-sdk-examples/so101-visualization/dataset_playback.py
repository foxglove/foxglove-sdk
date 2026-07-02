"""Replay a recorded LeRobot dataset episode in Foxglove as a seekable timeline.

LeRobot's native Foxglove integration includes seekable dataset playback built on the
Foxglove SDK's PlaybackControl capability: the Foxglove app drives play/pause/seek/speed,
and frames are read from the on-disk dataset on demand, stamped at their dataset
timestamps. This script shows how to use it programmatically; the equivalent CLI is:

    lerobot-dataset-viz --repo-id <repo_id> --episode-index 0 --display-mode foxglove

Requires the LeRobot dataset visualization extras: pip install 'lerobot[dataset_viz]'
"""

import argparse

from lerobot.datasets.lerobot_dataset import LeRobotDataset
from lerobot.utils.foxglove_visualization import serve_foxglove_dataset_playback


def parse_args():
    parser = argparse.ArgumentParser(
        description="Serve a LeRobot dataset episode to Foxglove with playback controls",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "--repo-id",
        type=str,
        default="lerobot/svla_so101_pickplace",
        help="Hugging Face dataset repository ID, e.g. one recorded with lerobot-record",
    )
    parser.add_argument(
        "--episode-index",
        type=int,
        default=0,
        help="Index of the episode to visualize",
    )
    parser.add_argument(
        "--root",
        type=str,
        default=None,
        help="Local directory containing the dataset (downloaded from the hub if omitted)",
    )
    parser.add_argument(
        "--host",
        type=str,
        default="127.0.0.1",
        help="Host interface to bind the Foxglove WebSocket server to",
    )
    parser.add_argument(
        "--port",
        type=int,
        default=8765,
        help="Port to bind the Foxglove WebSocket server to",
    )
    parser.add_argument(
        "--compress-images",
        action="store_true",
        help="JPEG-compress camera frames to save bandwidth",
    )
    parser.add_argument(
        "--no-autoplay",
        action="store_true",
        help="Wait for the user to press play in Foxglove instead of playing on connect",
    )
    return parser.parse_args()


def main():
    args = parse_args()

    print(f"Loading episode {args.episode_index} of {args.repo_id} ...")
    dataset = LeRobotDataset(
        args.repo_id, episodes=[args.episode_index], root=args.root
    )

    # Starts a Foxglove WebSocket server advertising the episode's time range via the
    # PlaybackControl capability, and blocks until interrupted. Data appears on the same
    # topics as the live integration: /observation/state, /action/state, and
    # /observation/images/<camera>.
    serve_foxglove_dataset_playback(
        dataset,
        args.episode_index,
        host=args.host,
        port=args.port,
        compress_images=args.compress_images,
        autoplay=not args.no_autoplay,
    )


if __name__ == "__main__":
    main()
