import importlib.metadata
import pathlib
from typing import Any, Optional

import anywidget
import traitlets

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
        orgSlug (str): Foxglove organization the user should be signed into.
        layout (dict): A custom layout configuration exported from the Foxglove app.

    Example:
        >>> import foxglove
        >>> buffer = foxglove.create_notebook_buffer()
        >>> # ... log some data to the buffer ...
        >>> viewer = foxglove.visualize(buffer, width="800px", height="600px")
        >>> viewer  # Display the widget in the notebook
    """

    _esm = pathlib.Path(__file__).parent / "static" / "widget.js"
    _data = traitlets.Bytes(b"").tag(sync=True)
    width = traitlets.Unicode("100%").tag(sync=True)
    height = traitlets.Unicode("500px").tag(sync=True)
    src = traitlets.Unicode("").tag(sync=True)
    orgSlug = traitlets.Unicode("").tag(sync=True)
    layout = traitlets.Dict({}).tag(sync=True)

    def __init__(
        self,
        datasource: NotebookBuffer,
        width: Optional[str] = None,
        height: Optional[str] = None,
        src: Optional[str] = None,
        orgSlug: Optional[str] = None,
        layout: Optional[dict] = None,
        **kwargs: Any,
    ):
        """
        Initialize the FoxgloveViewer widget with the specified data source and configuration.

        Args:
            datasource: The NotebookBuffer containing the data to visualize. This buffer
                should have been populated with logged messages before creating the viewer.
            width: Optional width for the widget. Can be specified as CSS values like
                "800px", "100%", "50vw", etc. If not provided, defaults to "100%".
            height: Optional height for the widget. Can be specified as CSS values like
                "600px", "80vh", "400px", etc. If not provided, defaults to "500px".
            src: Optional URL of the Foxglove app instance to use. If not provided or empty,
                uses the default Foxglove embed server (https://embed.foxglove.dev/).
            orgSlug: Optional Foxglove organization the user should be signed into.
            layout: Optional custom layout configuration. Should be a dictionary that
                was exported from the Foxglove app. If not provided, uses the default layout.
            **kwargs: Additional keyword arguments passed to the parent AnyWidget class.

        Note:
            The widget will automatically load the data from the provided datasource
            and display it in the embedded Foxglove viewer. The data is loaded once
            during initialization - to update the data, use the :meth:`set_datasource` method.

        Example:
            >>> import foxglove
            >>> buffer = foxglove.create_notebook_buffer()
            >>> # ... log some data to the buffer ...
            >>> viewer = FoxgloveViewer(
            ...     datasource=buffer,
            ...     width="800px",
            ...     height="600px",
            ...     orgSlug="my-org"
            ... )
        """
        super().__init__(**kwargs)

        if width is not None:
            self.width = width
        if height is not None:
            self.height = height
        if src is not None:
            self.src = src
        if orgSlug is not None:
            self.orgSlug = orgSlug
        if layout is not None:
            self.layout = layout

        self._data = datasource.get_data()

    def set_datasource(self, datasource: NotebookBuffer) -> None:
        """
        Update the data source for the Foxglove viewer widget.

        This method allows you to dynamically change the data being visualized
        without recreating the widget. The new data will be loaded from the
        provided NotebookBuffer and the viewer will update to display it.

        Args:
            datasource: The new NotebookBuffer containing the data to visualize.

        Example:
            >>> import foxglove
            >>> buffer1 = foxglove.create_notebook_buffer()
            >>> # ... log some initial data to buffer1 ...
            >>> viewer = foxglove.visualize(buffer1)
            >>>
            >>> # Later, update with new data
            >>> buffer2 = foxglove.create_notebook_buffer()
            >>> # ... log different data to buffer2 ...
            >>> viewer.set_datasource(buffer2)
        """
        self._data = datasource.get_data()
