import os
import uuid
from tempfile import TemporaryDirectory
from typing import Optional

from .._foxglove_py import Context, open_mcap


class NotebookBuffer:
    """
    A data buffer for collecting and managing messages in Jupyter notebooks.
    """

    def __init__(self, context: Optional[Context] = None):
        """
        Initialize a new NotebookBuffer for collecting logged messages.
        """
        # We need to keep the temporary directory alive until the writer is closed
        self._temp_directory = TemporaryDirectory()
        self._context = context
        self._create_writer()

    def get_data(self) -> bytes:
        """
        Retrieve all collected data and reset the buffer for new data collection.
        """
        self._writer.close()

        with open(self._file_name, "rb") as f_read:
            # Read the entire content of the file
            content = f_read.read()

        os.remove(self._file_name)
        self._create_writer()

        return content

    def _create_writer(self) -> None:
        random_id = uuid.uuid4().hex[:8]
        self._file_name = f"{self._temp_directory.name}/log-{random_id}.mcap"
        self._writer = open_mcap(path=self._file_name, context=self._context)
