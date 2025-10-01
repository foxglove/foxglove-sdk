import importlib.metadata
import pathlib
from typing import Any, Optional

import anywidget
import traitlets

from .._foxglove_py import Context
from .NotebookBuffer import NotebookBuffer

try:
    __version__ = importlib.metadata.version("foxglove")
except importlib.metadata.PackageNotFoundError:
    __version__ = "unknown"


class FoxgloveViewer(anywidget.AnyWidget):
    """
    A Jupyter notebook widget that embeds the Foxglove visualization app for interactive
    data exploration.

    This widget provides a fully-featured Foxglove interface directly within Jupyter notebooks,
    allowing you to visualize multi-modal robotics data including 3D scenes, plots, images,
    and more.

    Attributes:
        width (str): The width of the widget. Defaults to "100%".
        height (str): The height of the widget. Defaults to "500px".
        src (str): The URL of the Foxglove app instance to use.
            If empty, uses the default embed server.
        layout_data (dict): A custom layout configuration exported from the Foxglove app.

    Example:
        >>> import foxglove
        >>> foxglove.create_notebook_buffer()
        >>> # ... log some data to the buffer ...
        >>> viewer = foxglove.visualize(width="800px", height="600px")
        >>> viewer  # Display the widget in the notebook
    """

    _esm = pathlib.Path(__file__).parent / "static" / "widget.js"
    _data = traitlets.Bytes(b"").tag(sync=True)
    width = traitlets.Unicode("100%").tag(sync=True)
    height = traitlets.Unicode("500px").tag(sync=True)
    src = traitlets.Unicode("").tag(sync=True)
    layout_data = traitlets.Dict({}).tag(sync=True)
    # A mapping of Context to NotebookBuffer, shared across all instances
    _notebook_buffers_by_context: dict[Context, NotebookBuffer] = {}

    def __init__(
        self,
        context: Optional[Context] = None,
        width: Optional[str] = None,
        height: Optional[str] = None,
        src: Optional[str] = None,
        layout_data: Optional[dict] = None,
        **kwargs: Any,
    ):
        """
        Initialize the FoxgloveViewer widget with the specified data source and configuration.

        Args:
            context: The Context used to log the messages. If no Context is provided, the global
                context will be used. The visualization data will be retrieved from the
                NotebookBuffer associated with the provided context. This buffer should have been
                populated with logged messages before creating the viewer.
            width: Optional width for the widget. Can be specified as CSS values like
                "800px", "100%", "50vw", etc. If not provided, defaults to "100%".
            height: Optional height for the widget. Can be specified as CSS values like
                "600px", "80vh", "400px", etc. If not provided, defaults to "500px".
            src: Optional URL of the Foxglove app instance to use. If not provided or empty,
                uses the default Foxglove embed server (https://embed.foxglove.dev/).
            layout_data: Optional layout data to be used by the Foxglove viewer. Should be a
                dictionary that was exported from the Foxglove app. If not provided, uses the
                default layout.
            **kwargs: Additional keyword arguments passed to the parent AnyWidget class.

        Note:
            The widget will automatically load the data from the provided datasource
            and display it in the embedded Foxglove viewer. The data is loaded once
            during initialization - to update the data, use the :meth:`set_datasource` method.

        Example:
            >>> import foxglove
            >>> foxglove.create_notebook_buffer()
            >>> # ... log some data ...
            >>> viewer = FoxgloveViewer(
            ...     width="800px",
            ...     height="600px"
            ... )
        """
        super().__init__(**kwargs)

        if width is not None:
            self.width = width
        if height is not None:
            self.height = height
        if src is not None:
            self.src = src
        if layout_data is not None:
            self.layout_data = layout_data

        # Use default context if no context is provided
        ctx = context or Context.default()
        self.set_data_from_context(ctx)

    def set_data_from_context(self, context: Context) -> None:
        """
        Update the data visualized using the provided context.

        This method allows you to dynamically change the data being visualized
        without recreating the widget. The new data will be loaded from the
        NotebookBuffer associated with the provided context and the viewer will update to
        display it.

        Args:
            context: The Context used to log the messages. The visualization data will be retrieved
                from the NotebookBuffer associated with the provided context. This buffer should
                have been populated with logged messages before creating the viewer.

        Example:
            >>> import foxglove
            >>> ctx_1 = Context()
            >>> foxglove.create_notebook_buffer(context=ctx_1)
            >>> # ... log some data to ctx_1 ...
            >>> viewer.set_data_from_context(ctx_1)
            >>>
            >>> # Later, update with new data
            >>> ctx_2 = Context()
            >>> foxglove.create_notebook_buffer(context=ctx_2)
            >>> # ... log different data to buffer2 ...
            >>> viewer.set_datasource(ctx_2)
        """
        # Create a new buffer if one doesn't exist
        if context not in self._notebook_buffers_by_context:
            self._notebook_buffers_by_context[context] = NotebookBuffer(context=context)

        # Get the buffer associated with the context
        datasource = self._notebook_buffers_by_context[context]

        # Set the viewer data
        self._data = datasource.get_data()

    @classmethod
    def create_notebook_buffer(cls, context: Optional[Context] = None) -> None:
        """
        Create a data buffer for collecting messages in Jupyter notebooks. The buffer
        will be associated with the provided context, so every message logged to the context
        will be collected by the buffer.
        """
        # Use default context if no context is provided
        ctx = context or Context.default()
        # Create a new buffer if one doesn't exist
        if ctx not in cls._notebook_buffers_by_context:
            cls._notebook_buffers_by_context[ctx] = NotebookBuffer(context=ctx)
