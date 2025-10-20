from typing import Literal, Optional, Union

from .._foxglove_py import Context
from .foxglove_widget import FoxgloveWidget
from .notebook_buffer import NotebookBuffer


class NotebookSink:
    """
    A NotebookSink object to manage data buffering and visualization in Jupyter notebooks.

    The NotebookSink object will buffer all data logged to the provided context. When you
    are ready to visualize the data, you can call the :meth:`show` method to display an embedded
    Foxglove visualization widget. The widget provides a fully-featured Foxglove interface
    directly within your Jupyter notebook, allowing you to explore multi-modal robotics data
    including 3D scenes, plots, images, and more.

    :param context: The Context used to log the messages. If no Context is provided, the global
        context will be used. Logged messages will be buffered.
    """

    def __init__(
        self,
        context: Optional[Context] = None,
    ):
        self._buffer = NotebookBuffer(context=context)

    def show(
        self,
        width: Optional[Union[int, Literal["full"]]] = None,
        height: Optional[int] = None,
        src: Optional[str] = None,
        layout_data: Optional[dict] = None,
    ) -> FoxgloveWidget:
        """
        Show the Foxglove viewer. Call this method as the last step of a notebook cell
        to display the viewer.
        """
        widget = FoxgloveWidget(
            get_data=self._get_data,
            width=width,
            height=height,
            src=src,
            layout_data=layout_data,
        )
        return widget

    def clear(self) -> None:
        """
        Clear the buffered data.
        """
        self._buffer.clear()

    def _get_data(self) -> list[bytes]:
        return self._buffer.get_data()
