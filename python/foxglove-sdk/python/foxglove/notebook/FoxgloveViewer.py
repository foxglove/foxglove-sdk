import importlib.metadata
import pathlib
from typing import Any, Optional

import anywidget
import traitlets

from .NotebookBuffer import NotebookBuffer

try:
    __version__ = importlib.metadata.version("foxglove")
except importlib.metadata.PackageNotFoundError:
    __version__ = "unknown"


class FoxgloveViewer(anywidget.AnyWidget):
    """
    A jupyter notebook widget that allows you to visualize multi-modal data using Foxglove.
    """

    _esm = pathlib.Path(__file__).parent / "static" / "widget.js"
    _data = traitlets.Bytes(b"").tag(sync=True)
    width = traitlets.Unicode("100%").tag(sync=True)
    height = traitlets.Unicode("500px").tag(sync=True)
    src = traitlets.Unicode("https://embed.foxglove.dev/").tag(sync=True)
    orgSlug = traitlets.Unicode("").tag(sync=True)
    layout = traitlets.Dict({}).tag(sync=True)

    def __init__(
        self,
        width: Optional[str] = None,
        height: Optional[str] = None,
        src: Optional[str] = None,
        orgSlug: Optional[str] = None,
        layout: Optional[dict] = None,
        datasource: Optional[NotebookBuffer] = None,
        **kwargs: Any,
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
        if datasource is not None:
            self._data = datasource.get_data()

    def set_layout(self, layout: dict) -> None:
        """
        Set the layout to be used in the Foxglove app.
        """
        self.layout = layout

    def set_datasource(self, datasource: NotebookBuffer) -> None:
        """
        Set the data to be visualized in the Foxglove app.
        """
        self._data = datasource.get_data()
