import os
import uuid
from tempfile import TemporaryDirectory
from typing import Union

from .._foxglove_py import Context, open_mcap
from ..mcap import MCAPWriter


class NotebookBuffer:
    """
    A data buffer for collecting and managing messages in Jupyter notebooks.

    The NotebookBuffer provides a convenient way to collect logged messages and data
    that can be visualized using the FoxgloveViewer widget. It acts as an intermediate
    storage layer between your data logging code and the visualization widget.

    The buffer automatically manages data storage and provides methods to retrieve
    the collected data for visualization. It's designed to work seamlessly with
    the Foxglove SDK's logging functions and the FoxgloveViewer widget.

    Example:
        >>> import foxglove
        >>> from foxglove.schemas import SceneUpdate
        >>>
        >>> # Create a buffer
        >>> buffer = foxglove.create_notebook_buffer()
        >>>
        >>> # Log some data (this would typically be done by your application)
        >>> # The buffer automatically collects the logged messages
        >>>
        >>> # Create a viewer to visualize the data
        >>> viewer = foxglove.visualize(buffer)
    """

    _file_name: str
    _writer: MCAPWriter
    _context: Union[Context, None]
    # We need to keep the temporary directory alive until the writer is closed
    _temp_directory: TemporaryDirectory

    def __init__(self, context: Union[Context, None] = None):
        """
        Initialize a new NotebookBuffer for collecting logged messages.

        Args:
            context: Optional Foxglove context to use for logging. If not provided,
                the global context will be used. This allows you to isolate the
                buffer's data collection from other parts of your application.

        Note:
            The buffer is immediately ready to collect data after initialization.
            Any messages logged through the Foxglove SDK will be automatically
            collected by this buffer if it's associated with the active context.

        Example:
            >>> import foxglove
            >>>
            >>> # Create a buffer with the global context
            >>> buffer = foxglove.create_notebook_buffer()
            >>>
            >>> # Or create a buffer with a specific context
            >>> context = foxglove.Context()
            >>> buffer = foxglove.create_notebook_buffer(context=context)
        """
        self._temp_directory = TemporaryDirectory()
        self._context = context
        self._create_writer()

    def get_data(self) -> bytes:
        """
        Retrieve all collected data and reset the buffer for new data collection.

        This method returns all the data that has been collected by the buffer
        since its creation (or since the last call to get_data). After calling
        this method, the buffer is reset and ready to collect new data.

        Returns:
            bytes: The collected data in a format suitable for visualization
                with the FoxgloveViewer widget. The data includes all logged
                messages, schemas, and metadata.

        Note:
            This method is typically called automatically by the FoxgloveViewer
            widget when it needs to display the data. You generally don't need
            to call this method directly unless you're implementing custom
            visualization logic.

        Example:
            >>> import foxglove
            >>>
            >>> # Create a buffer and log some data
            >>> buffer = foxglove.create_notebook_buffer()
            >>> # ... your application logs data to the buffer ...
            >>>
            >>> # Retrieve the data for visualization
            >>> data = buffer.get_data()
            >>> print(f"Retrieved {len(data)} bytes of data")
            >>>
            >>> # The buffer is now reset and ready for new data
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
