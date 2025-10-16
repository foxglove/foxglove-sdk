import importlib.metadata
import pathlib
from typing import Any, Optional, Protocol

import anywidget
import traitlets

try:
    __version__ = importlib.metadata.version("foxglove")
except importlib.metadata.PackageNotFoundError:
    __version__ = "unknown"


class DataSource(Protocol):
    def __call__(self) -> list[bytes]: ...


class FoxgloveWidget(anywidget.AnyWidget):
    """
    A widget that displays a Foxglove viewer in a notebook.

    :param get_data: A callback function that returns the data to display in the widget.
    :param width: The width of the widget. Defaults to "100%".
    :param height: The height of the widget. Defaults to "500px".
    :param src: The source URL of the Foxglove viewer. Defaults to "https://embed.foxglove.dev/".
    :param layout_data: The layout data to use for the widget.
    """

    _esm = pathlib.Path(__file__).parent / "static" / "widget.js"
    width = traitlets.Unicode("100%").tag(sync=True)
    height = traitlets.Unicode("500px").tag(sync=True)
    src = traitlets.Unicode("").tag(sync=True)
    layout_data = traitlets.Dict({}).tag(sync=True)

    def __init__(
        self,
        get_data: DataSource,
        width: Optional[str] = None,
        height: Optional[str] = None,
        src: Optional[str] = None,
        layout_data: Optional[dict] = None,
        **kwargs: Any,
    ):
        super().__init__(**kwargs)
        if width is not None:
            self.width = width
        if height is not None:
            self.height = height
        if src is not None:
            self.src = src
        if layout_data is not None:
            self.layout_data = layout_data

        # Callback to get the data to display in the widget
        self._get_data = get_data
        # Keep track of when the widget is ready to receive data
        self._ready = False
        # Pending data to be sent when the widget is ready
        self._pending_data: list[bytes] = []
        self.on_msg(self._handle_custom_msg)
        self.refresh()

    def refresh(self) -> None:
        """
        Refresh the widget by getting the data from the callback function and sending it
        to the widget.
        """
        data = self._get_data()
        if not self._ready:
            self._pending_data = data
        else:
            self.send({"type": "update-data"}, data)

    def _handle_custom_msg(self, data: dict, buffers: list[bytes]) -> None:
        if data["type"] == "ready":
            self._ready = True

            if len(self._pending_data) > 0:
                self.send({"type": "update-data"}, self._pending_data)
                self._pending_data = []
