"""
This module defines types for programmatically constructing Foxglove `layouts <https://docs.foxglove.dev/docs/visualization/layouts>`_.

This API is currently experimental and not ready for public use.
"""


class Layout:
    """A Foxglove layout

    :raises NotImplementedError: This class is currently experimental and not ready for public use.
    """

    def to_json(self) -> str:
        raise NotImplementedError(
            "This class is currently experimental and not ready for public use."
        )
