from __future__ import annotations

import pathlib
from typing import TYPE_CHECKING, Any, Literal

import anywidget
import traitlets

if TYPE_CHECKING:
    from .notebook_buffer import NotebookBuffer


class FoxgloveWidget(anywidget.AnyWidget):
    """
    A widget that displays a Foxglove viewer in a notebook.

    :param buffer: The NotebookBuffer object that contains the data to display in the widget.
    :param layout_storage_key: The storage key of the layout to use for the widget.
    :param width: The width of the widget. Defaults to "full".
    :param height: The height of the widget in pixels. Defaults to 500.
    :param src: The source URL of the Foxglove viewer. Defaults to "https://embed.foxglove.dev/".
    """

    _esm = pathlib.Path(__file__).parent / "static" / "widget.js"
    width = traitlets.Union(
        [traitlets.Int(), traitlets.Enum(values=["full"])], default_value="full"
    ).tag(sync=True)
    height = traitlets.Int(default_value=500).tag(sync=True)
    src = traitlets.Unicode(default_value=None, allow_none=True).tag(sync=True)
    _layout_params = traitlets.Dict(
        per_key_traits={
            "storage_key": traitlets.Unicode(),
            "opaque_layout": traitlets.Dict(allow_none=True, default_value=None),
            "force": traitlets.Bool(False),
        },
        allow_none=True,
        default_value=None,
    ).tag(sync=True)

    def __init__(
        self,
        buffer: NotebookBuffer,
        layout_storage_key: str,
        width: int | Literal["full"] | None = None,
        height: int | None = None,
        src: str | None = None,
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

        self.select_layout(layout_storage_key, **kwargs)

        # Callback to get the data to display in the widget
        self._buffer = buffer
        # Keep track of when the widget is ready to receive data
        self._ready = False
        # Pending data to be sent when the widget is ready
        self._pending_data: list[bytes] = []
        self.on_msg(self._handle_custom_msg)
        self.refresh()

    def select_layout(self, storage_key: str, **kwargs: Any) -> None:
        """
        Select a layout in the Foxglove viewer.
        """
        opaque_layout = kwargs.get("opaque_layout", None)
        force_layout = kwargs.get("force_layout", False)

        self._layout_params = {
            "storage_key": storage_key,
            "opaque_layout": opaque_layout if isinstance(opaque_layout, dict) else None,
            "force": force_layout,
        }

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

    def _handle_custom_msg(self, msg: dict, buffers: list[bytes]) -> None:
        if msg["type"] == "ready":
            self._ready = True

            if len(self._pending_data) > 0:
                self.send({"type": "update-data"}, self._pending_data)
                self._pending_data = []
