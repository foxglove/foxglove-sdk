from __future__ import annotations

from typing import TYPE_CHECKING, Any, Literal

from ..notebook.notebook_buffer import NotebookBuffer

if TYPE_CHECKING:
    from .._foxglove_py import Context

try:
    import marimo as mo
except ImportError:
    raise ImportError(
        "marimo is required for MarimoBuffer. "
        'Install it with `pip install "foxglove-sdk[marimo]"`'
    )

from ..layouts import Layout
from .marimo_widget import _MarimoFoxgloveViewer


class MarimoBuffer:
    """
    A data buffer for collecting and visualizing Foxglove data in marimo notebooks.

    Wraps :class:`~foxglove.notebook.notebook_buffer.NotebookBuffer` and returns
    marimo-compatible widgets via ``mo.ui.anywidget()``.

    Example::

        import foxglove
        from foxglove.marimo import MarimoBuffer

        buf = foxglove.init_marimo_buffer()
        # ... log data ...
        buf.show()
    """

    def __init__(self, *, context: Context | None = None):
        """
        Initialize a new MarimoBuffer for collecting logged messages.

        :param context: The Context used to log the messages. If no Context is provided,
            the global context will be used.
        """
        self._notebook_buffer = NotebookBuffer(context=context)

    def show(
        self,
        *,
        width: int | Literal["full"] | None = None,
        height: int | None = None,
        src: str | None = None,
        layout: Layout | None = None,
        opaque_layout: dict[str, Any] | None = None,
    ) -> Any:
        """
        Show the Foxglove viewer as a marimo-compatible widget.

        Returns a ``mo.ui.anywidget`` wrapping the Foxglove viewer. Place this
        as the last expression in a marimo cell to display it.

        :param width: The width of the widget. Defaults to ``"full"``.
        :param height: The height of the widget in pixels. Defaults to 500.
        :param src: The source URL of the Foxglove viewer.
        :param layout: An optional Layout to use as the initial layout.
        :param opaque_layout: An opaque layout dict exported from Foxglove.
        :returns: A marimo anywidget wrapping the Foxglove viewer.
        """
        viewer = _MarimoFoxgloveViewer(
            buffer=self._notebook_buffer,
            width=width,
            height=height,
            src=src,
            layout=layout,
            opaque_layout=opaque_layout,
        )
        return mo.ui.anywidget(viewer)

    def clear(self) -> None:
        """Clear the buffered data."""
        self._notebook_buffer.clear()

    def refresh_widget(self, widget: Any) -> None:
        """
        Refresh a previously created widget with the latest buffered data.

        :param widget: The widget returned by :meth:`show`.
        """
        widget.widget.refresh()
