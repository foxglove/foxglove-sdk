"""
Convert `LeRobot <https://github.com/huggingface/lerobot>`__ datasets into MCAP files, one per
episode.

This module is only available when the ``lerobot`` extra is installed. Install it with
``pip install "foxglove-sdk[lerobot]"``.
"""

try:
    from ._dataset import (
        Episode,
        Feature,
        LeRobotDataset,
        UnsupportedDatasetError,
        VideoSegment,
        load_dataset,
    )
    from ._video import KeyframeError, UnsupportedVideoError
    from ._writer import (
        DEFAULT_EPISODE_GAP_S,
        DEFAULT_START_TIME,
        EpisodeWriter,
        WrittenEpisode,
    )
except ModuleNotFoundError as err:
    if err.name not in ("av", "pyarrow"):
        raise
    raise ImportError(
        'The "lerobot" feature is not installed. '
        'Install it with `pip install "foxglove-sdk[lerobot]"`'
    ) from err

__all__ = [
    "DEFAULT_EPISODE_GAP_S",
    "DEFAULT_START_TIME",
    "Episode",
    "EpisodeWriter",
    "Feature",
    "KeyframeError",
    "LeRobotDataset",
    "UnsupportedDatasetError",
    "UnsupportedVideoError",
    "VideoSegment",
    "WrittenEpisode",
    "load_dataset",
]
