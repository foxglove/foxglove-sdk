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
        self._files: list[str] = []
        self._create_writer()

    def get_data(self) -> bytes:
        """
        Retrieve all collected data and reset the buffer for new data collection.
        """
        # close the current writer
        self._writer.close()

        # read the content of the files
        contents: list[bytes] = []
        for file_name in self._files:
            with open(file_name, "rb") as f_read:
                contents.append(f_read.read())

        self._create_writer()

        bytes_contents: bytes = b""
        # merge the contents into a single bytes object
        if contents:
            # Use a unique separator that's unlikely to appear in the data
            # We'll use a 4-byte magic number followed by the file index
            separator = b"\x00\xFF\x00\xFF"  # Magic bytes for separator

            # First, add the number of files as a 4-byte integer
            file_count = len(contents).to_bytes(4, byteorder="big")
            bytes_contents = file_count

            # Then add each file with its separator
            for i, file_data in enumerate(contents):
                # Add separator with file index
                file_separator = separator + i.to_bytes(4, byteorder="big")
                bytes_contents += file_separator
                # Add the file size as 8-byte integer
                file_size = len(file_data).to_bytes(8, byteorder="big")
                bytes_contents += file_size
                # Add the actual file data
                bytes_contents += file_data

        return bytes_contents

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

    def _create_writer(self) -> None:
        random_id = uuid.uuid4().hex[:8]
        file_name = f"{self._temp_directory.name}/log-{random_id}.mcap"
        self._files.append(file_name)
        self._writer = open_mcap(path=file_name, context=self._context)
