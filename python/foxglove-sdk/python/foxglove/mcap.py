# Re-export these imports
from typing import TYPE_CHECKING, Any

from ._foxglove_py.mcap import (
    MCAPCompression,
    MCAPWriteOptions,
)
from ._foxglove_py.mcap import MCAPWriter as _MCAPWriter

if TYPE_CHECKING:
    from .layouts import Layout


class MCAPWriter:
    """
    A writer for logging messages to an MCAP file.

    Obtain an instance by calling :py:func:`foxglove.open_mcap`.

    This class may be used as a context manager, in which case the writer will
    be closed when you exit the context.

    If the writer is not closed by the time it is garbage collected, it will be
    closed automatically, and any errors will be logged.
    """

    def __init__(self, inner: _MCAPWriter) -> None:
        self._inner = inner

    def __enter__(self) -> "MCAPWriter":
        return self

    def __exit__(self, exc_type: Any, exc_value: Any, traceback: Any) -> None:
        self._inner.__exit__(exc_type, exc_value, traceback)

    def close(self) -> None:
        """
        Close the writer explicitly.

        You may call this to explicitly close the writer. Note that the writer
        will be automatically closed when it is garbage-collected, or when
        exiting the context manager.
        """
        self._inner.close()

    def write_metadata(self, name: str, metadata: dict[str, str]) -> None:
        """
        Write metadata to the MCAP file.

        Metadata consists of key-value string pairs associated with a name.
        If the metadata dictionary is empty, this method does nothing.

        :param name: Name identifier for this metadata record
        :param metadata: Dictionary of key-value pairs to store
        """
        self._inner.write_metadata(name, metadata)

    def attach(
        self,
        *,
        log_time: int,
        create_time: int,
        name: str,
        media_type: str,
        data: bytes,
    ) -> None:
        """
        Write an attachment to the MCAP file.

        Attachments are arbitrary binary data that can be stored alongside messages.
        Common uses include storing configuration files, calibration data, or other
        reference material related to the recording.

        :param log_time: Time at which the attachment was logged, in nanoseconds since
            epoch.
        :param create_time: Time at which the attachment data was created, in nanoseconds
            since epoch.
        :param name: Name of the attachment (e.g., "config.json").
        :param media_type: MIME type of the attachment (e.g., "application/json").
        :param data: Binary content of the attachment.
        """
        self._inner.attach(
            log_time=log_time,
            create_time=create_time,
            name=name,
            media_type=media_type,
            data=data,
        )

    def write_layout(self, layout: "Layout") -> None:
        """
        Write a layout to the MCAP file.

        The layout is serialized to JSON and stored in the file's metadata
        under the "foxglove.layout" key.

        :param layout: The Layout object to write to the MCAP file.
        """
        layout_json = layout.to_json()
        self._inner.write_metadata("foxglove.layout", {"": layout_json})


__all__ = [
    "MCAPCompression",
    "MCAPWriter",
    "MCAPWriteOptions",
]
