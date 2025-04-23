API Reference
=============

Version: |release|

foxglove
--------

.. automodule:: foxglove
   :members:
   :exclude-members: MCAPWriter

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

      A decimal or integer value that can be represented as a 64-bit floating point number.

   .. py:data:: Float64Array

      An array of decimal or integer values that can be represented as 64-bit floating point numbers.

.. autoclass:: foxglove.websocket.ParameterValue

   .. py:class:: Number(value: float)

      A decimal or integer value.

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
