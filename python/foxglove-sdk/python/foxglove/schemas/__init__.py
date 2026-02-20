"""
Deprecated module: Use `foxglove.messages` instead.

This module re-exports all types from `foxglove.messages` for backward compatibility.
"""

import warnings

warnings.warn(
    "foxglove.schemas is deprecated, use foxglove.messages instead",
    DeprecationWarning,
    stacklevel=2,
)

# Re-export everything from messages for backward compatibility.
from foxglove.messages import *  # noqa: F401, F403, E402
from foxglove.messages import __all__  # noqa: F401, E402
