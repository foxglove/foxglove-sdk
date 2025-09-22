import importlib.metadata
import pathlib
import uuid
from typing import Any, Optional

from tempfile import TemporaryDirectory
import anywidget
import traitlets
import foxglove

try:
    __version__ = importlib.metadata.version("foxglove_notebook")
except importlib.metadata.PackageNotFoundError:
    __version__ = "unknown"


class FoxgloveViewer(anywidget.AnyWidget):
    """
    A jupyter notebook widget that allows you to visualize multi-modal data in a Foxglove app.
    """

    _esm = pathlib.Path(__file__).parent / "static" / "widget.js"
    width = traitlets.Unicode("100%").tag(sync=True)
    height = traitlets.Unicode("500px").tag(sync=True)
    src = traitlets.Unicode("https://embed.foxglove.dev/").tag(sync=True)
    orgSlug = traitlets.Unicode("").tag(sync=True)
    data = traitlets.Bytes(b"").tag(sync=True)
    layout = traitlets.Dict({}).tag(sync=True)
    writer = None

    def __init__(
        self,
        width: Optional[str] = None,
        height: Optional[str] = None,
        src: Optional[str] = None,
        orgSlug: Optional[str] = None,
        layout: Optional[dict] = None,
        **kwargs: Any
    ):
        """
        Initialize the FoxgloveViewer widget and prepares it for logging
        """
        super().__init__(**kwargs)

        if width is not None:
            self.width = width
        if height is not None:
            self.height = height
        if src is not None:
            self.src = src
        if orgSlug is not None:
            self.orgSlug = orgSlug
        if layout is not None:
            self.layout = layout


        # Create a temporary directory where the MCAP file will be stored
        self.temp_directory = TemporaryDirectory()

        # Prepare the widget for logging
        self.start_logging()

    def set_layout(self, layout: dict) -> None:
        """
        Set the layout of the widget.
        """
        self.layout = layout

    def start_logging(self) -> None:
        """
        Prepare the widget for logging.
        """
        if self.writer is not None:
            # Close the previous writer if it exists
            self.writer.close()
            self.writer = None
            self.file_name = None

        random_id = uuid.uuid4().hex[:8]
        self.file_name = f"{self.temp_directory.name}/log-{random_id}.mcap"
        self.writer = foxglove.open_mcap(self.file_name)

    def show(self) -> None:
        """
        Show logged data using Foxglove.
        """
        if self.writer is None:
            raise Exception("Logging not started")

        self.writer.close()

        with open(self.file_name, 'rb') as f_read:
            # Read the entire content of the file
            content = f_read.read()
            self.data = content

        self.writer = None
        self.file_name = None

        return self
