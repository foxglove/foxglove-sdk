from typing import Optional

from .._foxglove_py import Context
from .notebook_buffer import NotebookBuffer
from .widget import Widget


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
        self._widget = Widget(
            width=width,
            height=height,
            src=src,
            layout_data=layout_data,
        )

    def show(self) -> Widget:
        """
        Show the Foxglove viewer. Call this method as the last step of a notebook cell
        to display the viewer.
        """
        self.reload_data()
        return self._widget

    def set_width(self, width: str) -> None:
        """
        Set the width of the Foxglove viewer.
        """
        self._widget.width = width

    def set_height(self, height: str) -> None:
        """
        Set the height of the Foxglove viewer.
        """
        self._widget.height = height

    def set_layout_data(self, layout_data: dict) -> None:
        """
        Set the layout data of the Foxglove viewer. `layout_data` should be a dictionary that was
        exported from the Foxglove app.
        """
        self._widget.layout_data = layout_data

    def reload_data(self) -> None:
        """
        Read the buffered data and set it to the Foxglove viewer to update the visualization.
        """
        self._widget.send_data(self._buffer.get_data())

    def clear_buffer(self) -> None:
        """
        Clear the buffered data.
        """
        self._buffer.clear()
