import importlib.metadata
import pathlib
from typing import Any, Optional

import anywidget
import traitlets

try:
    __version__ = importlib.metadata.version("foxglove")
except importlib.metadata.PackageNotFoundError:
    __version__ = "unknown"


class Widget(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "widget.js"
    data = traitlets.Bytes(b"").tag(sync=True)
    width = traitlets.Unicode("100%").tag(sync=True)
    height = traitlets.Unicode("500px").tag(sync=True)
    src = traitlets.Unicode("").tag(sync=True)
    layout_data = traitlets.Dict({}).tag(sync=True)

    def __init__(
        self,
        data: Optional[bytes] = None,
        width: Optional[str] = None,
        height: Optional[str] = None,
        src: Optional[str] = None,
        layout_data: Optional[dict] = None,
        **kwargs: Any,
    ):
        super().__init__(**kwargs)

        if width is not None:
            self.width = width
        if height is not None:
            self.height = height
        if src is not None:
            self.src = src
        if layout_data is not None:
            self.layout_data = layout_data
        if data is not None:
            self.data = data
