from __future__ import annotations

import pathlib
import warnings
from typing import TYPE_CHECKING, Any, Literal

import anywidget
import traitlets

if TYPE_CHECKING:
    from ..layouts import Layout
    from ..notebook.notebook_buffer import NotebookBuffer


class _MarimoFoxgloveViewer(anywidget.AnyWidget):
    """
    Foxglove viewer anywidget for use with marimo's ``mo.ui.anywidget()``.

    This is the same viewer as the Jupyter ``_FoxgloveViewer`` but without
    ipywidgets dependencies, making it suitable for marimo notebooks.
    """

    _esm = pathlib.Path(__file__).parent.parent / "notebook" / "static" / "widget.js"

    width = traitlets.Union(
        [traitlets.Int(), traitlets.Enum(values=["full"])], default_value="full"
    ).tag(sync=True)
    height = traitlets.Int(default_value=500).tag(sync=True)
    src = traitlets.Unicode(default_value=None, allow_none=True).tag(sync=True)
    _layout = traitlets.Unicode(default_value=None, allow_none=True).tag(sync=True)
    _opaque_layout = traitlets.Dict(allow_none=True, default_value=None).tag(sync=True)

    def __init__(
        self,
        *,
        buffer: NotebookBuffer,
        width: int | Literal["full"] | None = None,
        height: int | None = None,
        src: str | None = None,
        layout: Layout | None = None,
        opaque_layout: dict[str, Any] | None = None,
    ):
        super().__init__()

        if width is not None:
            self.width = width
        else:
            self.width = "full"
        if height is not None:
            self.height = height
        if src is not None:
            self.src = src

        if layout is not None and opaque_layout is not None:
            raise ValueError("Cannot specify both layout and opaque_layout")
        if layout is not None:
            self._layout = layout.to_json()
        elif opaque_layout is not None:
            self._opaque_layout = opaque_layout

        self._buffer = buffer
        self._ready = False
        self._pending_data: list[bytes] = []
        self.on_msg(self._handle_custom_msg)
        self.refresh()

    def refresh(self) -> None:
        """Refresh the viewer with the latest data from the buffer."""
        data = self._buffer._get_data()
        if not self._ready:
            self._pending_data = data
        else:
            self.send({"type": "update-data"}, data)

    def _handle_custom_msg(self, msg: dict, buffers: list[bytes]) -> None:  # type: ignore[type-arg]
        if msg["type"] == "ready":
            self._ready = True

            if len(self._pending_data) > 0:
                self.send({"type": "update-data"}, self._pending_data)
                self._pending_data = []
        elif msg["type"] == "error":
            warnings.warn(
                f"Foxglove viewer error: {msg['message']}", stacklevel=2
            )
