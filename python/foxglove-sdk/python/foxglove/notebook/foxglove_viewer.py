from typing import Optional

from .._foxglove_py import Context
from .foxglove_widget import FoxgloveWidget
from .notebook_buffer import NotebookBuffer


class FoxgloveViewer:
    """
    A FoxgloveViewer object to manage data buffering and visualization in Jupyter notebooks.

    The FoxgloveViewer object will buffer all data logged to the provided context. When you
    are ready to visualize the data, you can call the :meth:`show` method to display an embedded
    Foxglove visualization widget. The widget provides a fully-featured Foxglove interface
    directly within your Jupyter notebook, allowing you to explore multi-modal robotics data
    including 3D scenes, plots, images, and more.
    """

    def __init__(
        self,
        context: Optional[Context] = None,
        width: Optional[str] = None,
        height: Optional[str] = None,
        src: Optional[str] = None,
        layout_data: Optional[dict] = None,
    ):
        self._buffer = NotebookBuffer(context=context)
        self.src = src
        self.width = width
        self.height = height
        self.layout_data = layout_data

    def show(
        self,
        width: Optional[str] = None,
        height: Optional[str] = None,
        src: Optional[str] = None,
        layout_data: Optional[dict] = None,
    ) -> FoxgloveWidget:
        """
        Show the Foxglove viewer. Call this method as the last step of a notebook cell
        to display the viewer.
        """
        data = self.get_data()

        widget = FoxgloveWidget(
            width=width if width is not None else self.width,
            height=height if height is not None else self.height,
            src=src if src is not None else self.src,
            layout_data=layout_data if layout_data is not None else self.layout_data,
        )
        widget.send_data(data)
        return widget

    def get_data(self) -> list[bytes]:
        """
        Read the buffered data and set it to the Foxglove viewer to update the visualization.
        """
        return self._buffer.get_data()

    def clear(self) -> None:
        """
        Clear the buffered data.
        """
        self._buffer.clear()
