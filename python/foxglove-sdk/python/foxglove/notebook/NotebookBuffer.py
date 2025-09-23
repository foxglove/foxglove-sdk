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
    # We need to keep the temporary directory alive until the writer is closed
    _temp_directory: TemporaryDirectory

    def __init__(self, context: Union[Context, None] = None):
        self._temp_directory = TemporaryDirectory()
        random_id = uuid.uuid4().hex[:8]
        self._file_name = f"{self._temp_directory.name}/log-{random_id}.mcap"
        self._writer = open_mcap(path=self._file_name, context=context)

    def get_data(self) -> bytes:
        self._writer.close()

        with open(self._file_name, "rb") as f_read:
            # Read the entire content of the file
            content = f_read.read()

        return content
