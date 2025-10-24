from __future__ import annotations

import os
import uuid
from tempfile import TemporaryDirectory
from typing import Any, Literal

from mcap.reader import make_reader

from .._foxglove_py import Context, open_mcap
from .foxglove_widget import FoxgloveWidget


class NotebookBuffer:
    """
    A data buffer to collect and manage messages and visualization in Jupyter notebooks.

    The NotebookBuffer object will buffer all data logged to the provided context. When you
    are ready to visualize the data, you can call the :meth:`show` method to display an embedded
    Foxglove visualization widget. The widget provides a fully-featured Foxglove interface
    directly within your Jupyter notebook, allowing you to explore multi-modal robotics data
    including 3D scenes, plots, images, and more.

    :param context: The Context used to log the messages. If no Context is provided, the global
        context will be used. Logged messages will be buffered.
    """

    def __init__(self, context: Context | None = None):
        """
        Initialize a new NotebookBuffer for collecting logged messages.
        """
        # We need to keep the temporary directory alive until the writer is closed
        self._temp_directory = TemporaryDirectory()
        self._context = context
        self._files: list[str] = []
        self._create_writer()

    def show(
        self,
        layout_storage_key: str,
        width: int | Literal["full"] | None = None,
        height: int | None = None,
        src: str | None = None,
        **kwargs: Any,
    ) -> FoxgloveWidget:
        """
        Show the Foxglove viewer. Call this method as the last step of a notebook cell
        to display the viewer.
        """
        widget = FoxgloveWidget(
            buffer=self,
            width=width,
            height=height,
            src=src,
            layout_storage_key=layout_storage_key,
            **kwargs,
        )
        return widget

    def clear(self) -> None:
        """
        Clear the buffered data.
        """
        self._writer.close()
        # Delete the temporary directory and all its contents
        self._temp_directory.cleanup()
        # Reset files list
        self._files = []
        # Create a new temporary directory
        self._temp_directory = TemporaryDirectory()
        self._create_writer()

    def get_data(self) -> list[bytes]:
        """
        Retrieve all collected data.
        """
        # close the current writer
        self._writer.close()

        if len(self._files) > 1:
            if is_mcap_empty(self._files[-1]):
                # If the last file is empty, remove the last file since it won't add any new data
                # to the buffer
                os.remove(self._files[-1])
                self._files.pop()
            elif is_mcap_empty(self._files[0]):
                # If the first file is empty, remove the first file since it won't add any new data
                # to the buffer
                os.remove(self._files[0])
                self._files.pop(0)

        # read the content of the files
        contents: list[bytes] = []
        for file_name in self._files:
            with open(file_name, "rb") as f_read:
                contents.append(f_read.read())

        self._create_writer()

        return contents

    def _create_writer(self) -> None:
        random_id = uuid.uuid4().hex[:8]
        file_name = f"{self._temp_directory.name}/log-{random_id}.mcap"
        self._files.append(file_name)
        self._writer = open_mcap(path=file_name, context=self._context)


def is_mcap_empty(file_name: str) -> bool:
    with open(file_name, "rb") as f_read:
        iter = make_reader(f_read).iter_messages()
        is_empty = next(iter, None) is None

    return is_empty
