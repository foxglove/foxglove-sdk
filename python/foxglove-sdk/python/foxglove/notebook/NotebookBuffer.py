import os
import uuid
from tempfile import TemporaryDirectory
from typing import Union

from .._foxglove_py import Context, open_mcap
from ..mcap import MCAPWriter


class NotebookBuffer:
    """
    A buffer for logging messages to be used with the FoxgloveViewer widget in a notebook.
    """

    _file_name: str
    _writer: MCAPWriter
    _context: Union[Context, None]
    # We need to keep the temporary directory alive until the writer is closed
    _temp_directory: TemporaryDirectory

    def __init__(self, context: Union[Context, None] = None):
        self._temp_directory = TemporaryDirectory()
        self._context = context
        self._create_writer()

    def get_data(self) -> bytes:
        """
        Retrieve and return all buffered data as bytes, then reset the buffer for new data.
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
