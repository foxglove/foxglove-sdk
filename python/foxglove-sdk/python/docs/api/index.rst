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
   :exclude-members: MCAPWriter, create_notebook_buffer, visualize

Notebook Integration
^^^^^^^^^^^^^^^^^^^^

Functions and classes for integrating with Jupyter notebooks and creating interactive visualizations.

.. py:function:: create_notebook_buffer(context: Context | None = None) -> NotebookBuffer

   Create a data buffer for collecting messages in Jupyter notebooks.

   This function creates a NotebookBuffer that can collect logged messages
   and data for visualization with the FoxgloveViewer widget. The buffer
   acts as an intermediate storage layer between your data logging code
   and the visualization widget.

   :param context: Optional Foxglove context to use for logging. If not provided,
       the global context will be used. This allows you to isolate the
       buffer's data collection from other parts of your application.
   :return: A buffer instance ready to collect logged messages
       and data for visualization.

   .. note::
      This function is only available when the `notebook` extra package
      is installed.

.. py:function:: visualize(datasource: NotebookBuffer, width: Optional[str] = None, height: Optional[str] = None, src: Optional[str] = None, orgSlug: Optional[str] = None, layout: Optional[dict] = None) -> FoxgloveViewer

   Create a FoxgloveViewer widget for interactive data visualization in Jupyter notebooks.

   This function creates an embedded Foxglove visualization widget that displays
   the data collected in the provided NotebookBuffer. The widget provides a
   fully-featured Foxglove interface directly within your Jupyter notebook,
   allowing you to explore multi-modal robotics data including 3D scenes,
   plots, images, and more.

   :param datasource: The NotebookBuffer containing the data to visualize. This buffer
       should have been populated with logged messages before creating the viewer.
   :param width: Optional width for the widget. Can be specified as CSS values like
       "800px", "100%", "50vw", etc. If not provided, defaults to "100%".
   :param height: Optional height for the widget. Can be specified as CSS values like
       "600px", "80vh", "400px", etc. If not provided, defaults to "500px".
   :param src: Optional URL of the Foxglove app instance to use. If not provided or empty,
       uses the default Foxglove embed server (https://embed.foxglove.dev/).
   :param orgSlug: Optional Foxglove organization the user should be signed into.
   :param layout: Optional custom layout configuration. Should be a dictionary that
       was exported from the Foxglove app. If not provided, uses the default layout.
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

.. py:class:: NotebookBuffer

   A data buffer for collecting and managing messages in Jupyter notebooks.

   The NotebookBuffer provides a convenient way to collect logged messages and data
   that can be visualized using the FoxgloveViewer widget. It acts as an intermediate
   storage layer between your data logging code and the visualization widget.

   The buffer automatically manages data storage and provides methods to retrieve
   the collected data for visualization. It's designed to work seamlessly with
   the Foxglove SDK's logging functions and the FoxgloveViewer widget.

   .. py:method:: __init__(context: Union[Context, None] = None)

      Initialize a new NotebookBuffer for collecting logged messages.

      :param context: Optional Foxglove context to use for logging. If not provided,
          the global context will be used. This allows you to isolate the
          buffer's data collection from other parts of your application.

      .. note::
         The buffer is immediately ready to collect data after initialization.
         Any messages logged through the Foxglove SDK will be automatically
         collected by this buffer if it's associated with the active context.

   .. py:method:: get_data() -> bytes

      Retrieve all collected data and reset the buffer for new data collection.

      This method returns all the data that has been collected by the buffer
      since its creation (or since the last call to get_data). After calling
      this method, the buffer is reset and ready to collect new data.

      :return: The collected data in a format suitable for visualization
          with the FoxgloveViewer widget. The data includes all logged
          messages, schemas, and metadata.

      .. note::
         This method is typically called automatically by the FoxgloveViewer
         widget when it needs to display the data. You generally don't need
         to call this method directly unless you're implementing custom
         visualization logic.

.. py:class:: FoxgloveViewer

   A Jupyter notebook widget that embeds the Foxglove visualization app for interactive data exploration.

   This widget provides a fully-featured Foxglove interface directly within Jupyter notebooks,
   allowing you to visualize multi-modal robotics data including 3D scenes, plots, images,
   and more.

   .. py:attribute:: width

      The width of the widget. Defaults to "100%". Can be specified as CSS values like
      "800px", "100%", "50vw", etc.

   .. py:attribute:: height

      The height of the widget. Defaults to "500px". Can be specified as CSS values like
      "600px", "80vh", "400px", etc.

   .. py:attribute:: src

      The URL of the Foxglove app instance to use. If empty, uses the default embed server
      (https://embed.foxglove.dev/).

   .. py:attribute:: orgSlug

      Foxglove organization the user should be signed into.

   .. py:attribute:: layout

      A custom layout configuration exported from the Foxglove app.

   .. py:method:: __init__(datasource: NotebookBuffer, width: Optional[str] = None, height: Optional[str] = None, src: Optional[str] = None, orgSlug: Optional[str] = None, layout: Optional[dict] = None, **kwargs: Any)

      Initialize the FoxgloveViewer widget with the specified data source and configuration.

      :param datasource: The NotebookBuffer containing the data to visualize. This buffer
          should have been populated with logged messages before creating the viewer.
      :param width: Optional width for the widget. Can be specified as CSS values like
          "800px", "100%", "50vw", etc. If not provided, defaults to "100%".
      :param height: Optional height for the widget. Can be specified as CSS values like
          "600px", "80vh", "400px", etc. If not provided, defaults to "500px".
      :param src: Optional URL of the Foxglove app instance to use. If not provided or empty,
          uses the default Foxglove embed server (https://embed.foxglove.dev/).
      :param orgSlug: Optional Foxglove organization the user should be signed into.
      :param layout: Optional custom layout configuration. Should be a dictionary that
          was exported from the Foxglove app. If not provided, uses the default layout.
      :param kwargs: Additional keyword arguments passed to the parent AnyWidget class.

      .. note::
         The widget will automatically load the data from the provided datasource
         and display it in the embedded Foxglove viewer. The data is loaded once
         during initialization - to update the data, use the :meth:`set_datasource` method.

   .. py:method:: set_datasource(datasource: NotebookBuffer) -> None

      Update the data source for the Foxglove viewer widget.

      This method allows you to dynamically change the data being visualized
      without recreating the widget. The new data will be loaded from the
      provided NotebookBuffer and the viewer will update to display it.

      :param datasource: The new NotebookBuffer containing the data to visualize.

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
