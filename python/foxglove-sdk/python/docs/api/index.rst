API Reference
=============

Version: |release|

.. note::
   The notebook integration classes and functions are only available when the `notebook` extra package is installed.
   Install it with `pip install foxglove-sdk[notebook]`.

foxglove
--------

.. automodule:: foxglove
   :members:
   :exclude-members: MCAPWriter, create_notebook_buffer, notebook_viewer

Notebook Integration
^^^^^^^^^^^^^^^^^^^^

Functions and classes for integrating with Jupyter notebooks and creating interactive visualizations.

.. py:function:: notebook_viewer(context: Optional[Context] = None, width: Optional[str] = None, height: Optional[str] = None, src: Optional[str] = None, layout_data: Optional[dict] = None) -> FoxgloveViewer

   Create a FoxgloveViewer object to manage data buffering and visualization in Jupyter
   notebooks.

   The FoxgloveViewer object will buffer all data logged to the provided context. When you
   are ready to visualize the data, you can call the :meth:`show` method to display an embedded
   Foxglove visualization widget. The widget provides a fully-featured Foxglove interface
   directly within your Jupyter notebook, allowing you to explore multi-modal robotics data
   including 3D scenes, plots, images, and more.

   :param context: The Context used to log the messages. If no Context is provided, the global
       context will be used. Logged messages will be buffered.
   :param width: Optional width for the widget. Can be specified as CSS values like
       "800px", "100%", "50vw", etc. If not provided, defaults to "100%".
   :param height: Optional height for the widget. Can be specified as CSS values like
       "600px", "80vh", "400px", etc. If not provided, defaults to "500px".
   :param src: Optional URL of the Foxglove app instance to use. If not provided or empty,
       uses the default Foxglove embed server (https://embed.foxglove.dev/).
   :param layout_data: Optional layout data to be used by the Foxglove viewer. Should be a
       dictionary that was exported from the Foxglove app. If not provided, uses the
       default layout.
   :return: A Jupyter widget that displays the embedded Foxglove
       visualization interface with the provided data.
   :raises Exception: If the notebook extra package is not installed. Install it
       with `pip install foxglove-sdk[notebook]`.

   .. note::
      This function is only available when the `notebook` extra package
      is installed. Install it with `pip install foxglove-sdk[notebook]`.
      The widget will automatically load the data from the provided datasource
      and display it in the embedded Foxglove viewer.

Notebook Classes
^^^^^^^^^^^^^^^^

.. py:class:: FoxgloveViewer

   A FoxgloveViewer object to manage data buffering and visualization in Jupyter notebooks.

   The FoxgloveViewer object will buffer all data logged to the provided context. When you
   are ready to visualize the data, you can call the :meth:`show` method to display an embedded
   Foxglove visualization widget. The widget provides a fully-featured Foxglove interface
   directly within your Jupyter notebook, allowing you to explore multi-modal robotics data
   including 3D scenes, plots, images, and more.

   .. py:method:: __init__(context: Optional[Context], width: Optional[str] = None, height: Optional[str] = None, src: Optional[str] = None, layout_data: Optional[dict] = None, **kwargs: Any)

      Initialize the FoxgloveViewer widget with the specified data source and configuration.

      :param context: The Context used to log the messages. If no Context is provided, the global
          context will be used. Logged messages will be buffered.
      :param width: Optional width for the widget. Can be specified as CSS values like
          "800px", "100%", "50vw", etc. If not provided, defaults to "100%".
      :param height: Optional height for the widget. Can be specified as CSS values like
          "600px", "80vh", "400px", etc. If not provided, defaults to "500px".
      :param src: Optional URL of the Foxglove app instance to use. If not provided or empty,
          uses the default Foxglove embed server (https://embed.foxglove.dev/).
      :param layout_data: Optional layout data to be used by the Foxglove viewer. Should be a
          dictionary that was exported from the Foxglove app. If not provided, uses the
          default layout.
      :param kwargs: Additional keyword arguments passed to the parent AnyWidget class.

   .. py:method:: show() -> Widget

      Show the Foxglove viewer. Call this method as the last step of a notebook cell
      to display the viewer.

   .. py:method:: set_width(width: str) -> None

      Set the width of the Foxglove viewer.

   .. py:method:: set_height(height: str) -> None

      Set the height of the Foxglove viewer.

   .. py:method:: set_layout_data(layout_data: dict) -> None

      Set the layout data of the Foxglove viewer.

   .. py:method:: reload_data() -> None

      Read the buffered data and set it to the Foxglove viewer to update the visualization.

   .. py:method:: clear_buffer() -> None

      Clear the buffered data.

Schemas
^^^^^^^

.. toctree::
   :maxdepth: 1

   ./schemas


Channels
^^^^^^^^

.. toctree::
   :maxdepth: 1

   ./channels

Parameters
^^^^^^^^^^

Used with the parameter service during live visualization. Requires the :py:data:`websocket.Capability.Parameters` capability.

.. autoclass:: foxglove.websocket.ParameterType

   .. py:data:: ByteArray

      A byte array.

   .. py:data:: Float64

      A floating-point value that can be represented as a 64-bit floating point number.

   .. py:data:: Float64Array

      An array of floating-point values that can be represented as 64-bit floating point numbers.

.. autoclass:: foxglove.websocket.ParameterValue

   .. py:class:: Float64(value: float)

     A floating-point value.

   .. py:class:: Integer(value: int)

      An integer value.

   .. py:class:: Bool(value: bool)

      A boolean value.

   .. py:class:: String(value: str)

      A string value.

      For parameters of type :py:attr:`ParameterType.ByteArray`, this is a
      base64 encoding of the byte array.

   .. py:class:: Array(value: list[ParameterValue])

      An array of parameter values.

   .. py:class:: Dict(value: dict[str, ParameterValue])

      An associative map of parameter values.

Asset handling
^^^^^^^^^^^^^^

You can provide an optional :py:class:`AssetHandler` to :py:func:`start_server` to serve assets such
as URDFs for live visualization. The asset handler is a :py:class:`Callable` that returns the asset
for a given URI, or None if it doesn't exist.

Foxglove assets will be requested with the `package://` scheme.
See https://docs.foxglove.dev/docs/visualization/panels/3d#resolution-of-urdf-assets-with-package-urls

This handler will be run on a separate thread; a typical implementation will load the asset from
disk and return its contents.

See the Asset Server example for more information.

.. autoclass:: foxglove.AssetHandler

.. py:class:: MCAPCompression

   Deprecated. Use :py:class:`mcap.MCAPCompression` instead.


foxglove.mcap
------------------

.. Enums are excluded and manually documented, since pyo3 only emulates them. (https://github.com/PyO3/pyo3/issues/2887)
.. Parameter types and values are manually documented since nested classes (values) are not supported by automodule.
.. automodule:: foxglove.mcap
   :members:
   :exclude-members: MCAPCompression

.. py:enum:: MCAPCompression

   .. py:data:: Zstd
   .. py:data:: Lz4


foxglove.websocket
------------------

.. Enums are excluded and manually documented, since pyo3 only emulates them. (https://github.com/PyO3/pyo3/issues/2887)
.. Parameter types and values are manually documented since nested classes (values) are not supported by automodule.
.. automodule:: foxglove.websocket
   :members:
   :exclude-members: Capability, ParameterType, ParameterValue, StatusLevel


Enums
^^^^^

.. py:enum:: Capability

   An enumeration of capabilities that you may choose to support for live visualization.

   Specify the capabilities you support when calling :py:func:`foxglove.start_server`. These will be
   advertised to the Foxglove app when connected as a WebSocket client.

   .. py:data:: ClientPublish

      Allow clients to advertise channels to send data messages to the server.

   .. py:data:: Parameters

      Allow clients to get & set parameters.

   .. py:data:: Services

      Allow clients to call services.

   .. py:data:: Time

      Inform clients about the latest server time.

      This allows accelerated, slowed, or stepped control over the progress of time. If the
      server publishes time data, then timestamps of published messages must originate from the
      same time source.

.. py:enum:: StatusLevel

   A level for :py:meth:`WebSocketServer.publish_status`.

   .. py:data:: Info
   .. py:data:: Warning
   .. py:data:: Error
