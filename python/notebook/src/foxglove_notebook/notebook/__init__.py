import importlib.metadata
import pathlib

from tempfile import TemporaryDirectory
import anywidget
import traitlets
import foxglove

try:
    __version__ = importlib.metadata.version("foxglove_notebook")
except importlib.metadata.PackageNotFoundError:
    __version__ = "unknown"


class FoxgloveViewer(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "widget.js"
    width = traitlets.Unicode("100%").tag(sync=True)
    height = traitlets.Unicode("500px").tag(sync=True)
    src = traitlets.Unicode("https://embed.foxglove.dev/").tag(sync=True)
    orgSlug = traitlets.Unicode("").tag(sync=True)
    data = traitlets.Bytes(b"").tag(sync=True)
    layout = traitlets.Dict({}).tag(sync=True)


    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.temp_directory = None
        if "width" in kwargs:
            self.width = kwargs["width"]
        if "height" in kwargs:
            self.height = kwargs["height"]
        if "src" in kwargs:
            self.src = kwargs["src"]
        if "orgSlug" in kwargs:
            self.orgSlug = kwargs["orgSlug"]

        if self.temp_directory is None:
            self.temp_directory = TemporaryDirectory()

        self.file_name = f"{self.temp_directory.name}/quickstart-python.mcap"
        self.writer = foxglove.open_mcap(self.file_name)

    def show(self):
        self.writer.close()

        with open(self.file_name, 'rb') as f_read:
            # Read the entire content of the file
            content = f_read.read()
            self.data = content

        self.temp_directory.cleanup()
        self.temp_directory = None
        self.writer = None
        self.file_name = None

        return self
