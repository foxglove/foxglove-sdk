.. role:: python(code)
   :language: python


Foxglove SDK documentation
==========================

Version: |release|

The official `Foxglove <https://docs.foxglove.dev/docs>`_ SDK for Python.

This package provides support for integrating with the Foxglove platform. It can be used to log
events to local `MCAP <https://mcap.dev/>`_ files or a local visualization server that communicates
with the Foxglove app.


Overview
--------

To record messages, you need at least one sink and at least one channel.

A "sink" is a destination for logged messages — either an MCAP file or a live visualization server.
Use :py:func:`.open_mcap` to register a new MCAP sink. Use
:py:func:`.start_server` to create a new live visualization server.

A "channel" gives a way to log related messages which have the same schema. Each channel is
instantiated with a unique topic name.

The SDK provides classes for well-known schemas. These can be used in conjunction with associated
channel classes for type-safe logging, which ensures at compile time that messages logged to a
channel all share a common schema. For example, you may create a :py:class:`.channels.SceneUpdateChannel` on
which you will log :py:class:`.schemas.SceneUpdate` messages.

You can also log messages with arbitrary schemas and provide your own encoding, by instantiating a
:py:class:`.Channel` class.


.. toctree::
   :maxdepth: 3

   examples
   api/index
