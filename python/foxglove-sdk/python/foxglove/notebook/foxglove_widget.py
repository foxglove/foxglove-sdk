from __future__ import annotations

import pathlib
from typing import TYPE_CHECKING, Any, Callable, Literal

import anywidget
import ipywidgets  # type: ignore
import traitlets

if TYPE_CHECKING:
    from ..layouts import Layout
    from .notebook_buffer import NotebookBuffer


class _FoxgloveViewer(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "widget.js"
    width = traitlets.Union(
        [traitlets.Int(), traitlets.Enum(values=["full"])], default_value="full"
    ).tag(sync=True)
    height = traitlets.Int(default_value=500).tag(sync=True)
    src = traitlets.Unicode(default_value=None, allow_none=True).tag(sync=True)
    _layout = traitlets.Unicode(default_value=None, allow_none=True).tag(sync=True)
    _opaque_layout = traitlets.Dict(allow_none=True, default_value=None).tag(sync=True)
    _on_error: Callable[[str], None]

    def __init__(
        self,
        *,
        on_error: Callable[[str], None],
        buffer: NotebookBuffer,
        width: int | Literal["full"] | None = None,
        height: int | None = None,
        src: str | None = None,
        layout: Layout | None = None,
        opaque_layout: dict[str, Any] | None = None,
    ):
        super().__init__(
            layout=ipywidgets.Layout(
                border="var(--jp-border-width, 1px) solid "
                + "var(--jp-cell-editor-border-color, #d5d5d5)"
            ),
        )
        self._on_error = on_error
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

        # Callback to get the data to display in the widget
        self._buffer = buffer
        # Keep track of when the widget is ready to receive data
        self._ready = False
        # Pending data to be sent when the widget is ready
        self._pending_data: list[bytes] = []
        self.on_msg(self._handle_custom_msg)
        self.refresh()

    def refresh(self) -> None:
        data = self._buffer._get_data()
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
        elif msg["type"] == "error":
            self._on_error(msg["message"])


class FoxgloveWidget(ipywidgets.VBox):
    """
    A widget that displays a Foxglove viewer in a notebook.
    """

    # Output widget for displaying error messages
    _output: ipywidgets.Output
    _viewer: _FoxgloveViewer

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
        """
        :param buffer: The NotebookBuffer object that contains the data to display in the widget.
        :param width: The width of the widget. Defaults to "full".
        :param height: The height of the widget in pixels. Defaults to 500.
        :param src: The source URL of the Foxglove viewer. Defaults to
            "https://embed.foxglove.dev/".
        :param opaque_layout: The layout data to load. This is an opaque dict object, which should
          be parsed from a JSON layout file that was exported from the Foxglove app. If not
          provided, the default layout will be used.
        """
        self._output = ipywidgets.Output(
            # Remove the default margin to avoid extra space when there are no error messages
            layout=ipywidgets.Layout(margin="0")
        )
        self._viewer = _FoxgloveViewer(
            on_error=self._output.append_stderr,
            buffer=buffer,
            width=width,
            height=height,
            src=src,
            layout=layout,
            opaque_layout=opaque_layout,
        )
        super().__init__(
            [
                self._viewer,
                self._output,
            ]
        )

    def refresh(self) -> None:
        """
        Refresh the widget by getting the data from the callback function and sending it
        to the widget.
        """
        self._viewer.refresh()
