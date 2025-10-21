from __future__ import annotations

import importlib.metadata
import pathlib
from typing import TYPE_CHECKING, Any, Literal, TypedDict

import anywidget
import traitlets

if TYPE_CHECKING:
    from .notebook_buffer import NotebookBuffer

try:
    __version__ = importlib.metadata.version("foxglove")
except importlib.metadata.PackageNotFoundError:
    __version__ = "unknown"


class SelectLayoutParams(TypedDict):
    storageKey: str
    opaqueLayout: dict
    force: bool | None


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
    width = traitlets.Union([traitlets.Int(), traitlets.Enum(values=["full"])]).tag(
        sync=True
    )
    height = traitlets.Int(500).tag(sync=True)
    src = traitlets.Unicode("").tag(sync=True)
    layout = traitlets.Dict(
        per_key_traits={
            "storageKey": traitlets.Unicode(),
            "opaqueLayout": traitlets.Dict(),
            "force": traitlets.Bool(False),
        }
    ).tag(sync=True)

    def __init__(
        self,
        buffer: NotebookBuffer,
        width: int | Literal["full"] | None = None,
        height: int | None = None,
        src: str | None = None,
        layout: SelectLayoutParams | None = None,
        **kwargs: Any,
    ):
        super().__init__(**kwargs)
        if width is not None:
            self.width = width
        else:
            self.width = "full"
        if height is not None:
            self.height = height
        if src is not None:
            self.src = src
        if layout is not None:
            self.layout = layout  # type: ignore[assignment]

        # Callback to get the data to display in the widget
        self._buffer = buffer
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
        data = self._buffer.get_data()
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
