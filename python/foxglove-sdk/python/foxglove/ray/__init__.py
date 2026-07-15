"""
Ray Data integration for Foxglove.

This module provides :class:`~foxglove.ray.datasource.FoxgloveDataset`, a `Ray Data
<https://docs.ray.io/en/latest/data/data.html>`__ datasource that downloads recordings from the
Foxglove cloud in parallel across a Ray cluster.

This module is only available when the ``ray`` extra is installed. Install it with
``pip install "foxglove-sdk[ray]"``.
"""

from __future__ import annotations

try:
    from .datasource import FoxgloveDataset
except ImportError as e:
    raise ImportError(
        'The "ray" feature is not installed. '
        'Install it with `pip install "foxglove-sdk[ray]"`'
    ) from e

__all__ = ["FoxgloveDataset"]
