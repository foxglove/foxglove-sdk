.. role:: python(code)
   :language: python


Foxglove SDK documentation
==========================

Version: |release|

The official `Foxglove <https://docs.foxglove.dev/docs>`_ SDK for Python.

This package provides support for integrating with the Foxglove platform. It can be used to log
events to local `MCAP <https://mcap.dev/>`_ files or a local visualization server that communicates
with the Foxglove app.

Getting started
---------------

Install the ``foxglove-sdk`` package from `PyPI <https://pypi.org/project/foxglove-sdk/>`_.

This will depend on your package manager. Our `examples
<https://github.com/foxglove/foxglove-sdk/tree/main/python/foxglove-sdk-examples>`_ use `poetry
<https://python-poetry.org/>`_.

To record messages, you need to initialize either an MCAP file writer or a WebSocket server for
live visualization.

For a hands-on walk-through, see https://docs.foxglove.dev/docs/sdk/example?lang=python.

Concepts
--------

Context
^^^^^^^

A :py:class:`.Context` is the binding between channels and sinks. Each channel and sink belongs to
exactly one context. Sinks receive advertisements about channels on the context, and can optionally
subscribe to receive logged messages on those channels.

When the context goes out of scope, its corresponding channels and sinks will be disconnected from
one another, and logging will stop. Attempts to log further messages on the channels will elicit
throttled warning messages.

Since many applications only need a single context, the SDK provides a static default context for
convenience.


Channels
^^^^^^^^

A :py:class:`.Channel` gives a way to log related messages which have the same type, or
:py:class:`.Schema`. Each channel is instantiated with a unique "topic", or name, which is typically
prefixed by a `/`. If you're familiar with MCAP, it's the same concept as an `MCAP channel <https://mcap.dev/guides/concepts#channel>`_.

A channel is always associated with exactly one :py:class:`.Context` throughout its lifecycle. The
channel remains attached to the context until it is either explicitly closed with
`Channel.close`, or the context is dropped. Attempting to log a message on a closed channel
will elicit a throttled warning.

Schemas
^^^^^^^

The SDK provides classes for well-known schemas. These can be used in conjunction with associated
channel classes for type-safe logging, which ensures at compile time that messages logged to a
channel all share a common schema. For example, you may create a :py:class:`.channels.SceneUpdateChannel` on
which you will log :py:class:`.schemas.SceneUpdate` messages. Note that the schema classes
are currently immutable and do not expose getters and setters for their fields. This is a limitation
we plan to address in the future.

You can also log messages with arbitrary schemas and provide your own encoding, by instantiating a
:py:class:`.Channel` class.

Sinks
^^^^^

A "sink" is a destination for logged messages. If you do not configure a sink, log messages will
simply be dropped without being recorded. You can configure multiple sinks, and you can create
or destroy them dynamically at runtime.

A sink is typically associated with exactly one :py:class:`.Context` throughout its lifecycle.
Details about how the sink is registered and unregistered from the context are sink-specific.

To create an MCAP file sink, use :py:func:`.open_mcap` and keep a reference to the returned handle.
As long as the handle remains in scope, events will be logged to the MCAP file. When the handle is
closed or dropped, the sink will be unregistered from the :py:class:`.Context`, and the file will be
finalized and flushed.

To create a live visualization server sink, use :py:func:`.start_server`. By default, the server
listens on ``127.0.0.1:8765``. Each client that connects to the websocket server is its own independent
sink. The sink is dynamically added to the :py:class:`.Context` associated with the server when the client
connects, and removed from the context when the client disconnects.


.. toctree::
   :maxdepth: 3

   examples
   api/index
