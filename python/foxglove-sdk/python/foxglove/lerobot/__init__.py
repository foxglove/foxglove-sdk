"""
Convert `LeRobot <https://github.com/huggingface/lerobot>`__ datasets into MCAP files, one per
episode.

This module is only available when the ``lerobot`` extra is installed. Install it with
``pip install "foxglove-sdk[lerobot]"``.
"""

try:
    from ._dataset import (
        DatasetMetadata,
        Episode,
        Feature,
        UnsupportedDatasetError,
        VideoSegment,
        load_metadata,
    )
    from ._video import (
        BFrameWarning,
        DepthMapWarning,
        UnsupportedCodecWarning,
        UnsupportedVideoError,
    )
    from ._writer import (
        EpisodeWriter,
    )
except ModuleNotFoundError as err:
    if err.name not in ("av", "pyarrow"):
        raise
    raise ImportError(
        'The "lerobot" feature is not installed. '
        'Install it with `pip install "foxglove-sdk[lerobot]"`'
    ) from err

__all__ = [
    "BFrameWarning",
    "DatasetMetadata",
    "DepthMapWarning",
    "Episode",
    "EpisodeWriter",
    "Feature",
    "UnsupportedCodecWarning",
    "UnsupportedDatasetError",
    "UnsupportedVideoError",
    "VideoSegment",
    "load_metadata",
]
