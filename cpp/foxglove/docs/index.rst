Foxglove SDK documentation
==========================

Version: |release|

The official `Foxglove <https://docs.foxglove.dev/docs>`_ SDK for C++.

This library provides support for integrating with the Foxglove platform. It can be used to log
events to local `MCAP <https://mcap.dev/>`_ files or a local visualization server that communicates
with the Foxglove app.

Installation
------------

The SDK is a wrapper around a C library. To build it, you will need to link that library and
compile the SDK source as part of your build process. The SDK assumes C++17 or newer.

Download the library, source, and header files for your platform from the
`SDK release <https://github.com/foxglove/foxglove-sdk/releases?q=sdk&expanded=true>`_ assets.

For a hands-on walk-through using CMake, see https://docs.foxglove.dev/docs/sdk/example?lang=cpp.

Overview
--------

To record messages, you need at least one sink and at least one channel.

A "sink" is a destination for logged messages — either an MCAP file or a live visualization server.
Use :func:`foxglove::McapWriter::create` to create a new MCAP sink. Use
:func:`foxglove::WebSocketServer::create` to create a new live visualization server.

A "channel" gives a way to log related messages which have the same schema. Each channel is
instantiated with a unique topic name.

You can log messages with arbitrary schemas and provide your own encoding, by instantiating a
:class:`foxglove::RawChannel`.

Thread safety
-------------

Sinks, channels, and contexts are thread-safe. Sinks and channels can be created concurrently
and shared or moved between threads. Logging is atomic and thread-safe.

Other types in the SDK are not thread-safe.

Concepts
--------

Context
^^^^^^^

A :class:`foxglove::Context` is the binding between channels and sinks. Each channel and sink belongs to
exactly one context. Sinks receive advertisements about channels on the context, and can optionally
subscribe to receive logged messages on those channels.

When the context goes out of scope, its corresponding channels and sinks will be disconnected from
one another, and logging will stop. Attempts to log further messages on the channels will elicit
throttled warning messages.

Since many applications only need a single context, the SDK provides a static default context for
convenience.


Channels
^^^^^^^^

A :class:`foxglove::RawChannel` gives a way to log related messages which have the same type, or
:class:`foxglove::Schema`. Each channel is instantiated with a unique "topic", or name, which is typically
prefixed by a `/`. If you're familiar with MCAP, it's the same concept as an [MCAP channel](https://mcap.dev/guides/concepts#channel).

A channel is always associated with exactly one :class:`foxglove::Context` throughout its lifecycle. The
channel remains attached to the context until it is either explicitly closed with
`close`, or the context is dropped. Attempting to log a message on a closed channel
will elicit a throttled warning.

Sinks
^^^^^

A "sink" is a destination for logged messages. If you do not configure a sink, log messages will
simply be dropped without being recorded. You can configure multiple sinks, and you can create
or destroy them dynamically at runtime.

A sink is typically associated with exactly one :class:`foxglove::Context` throughout its lifecycle.
Details about how the sink is registered and unregistered from the context are sink-specific.

To create an MCAP file sink, call :func:`foxglove::McapWriter::create` and keep a reference to the returned handle.
As long as the handle remains in scope, events will be logged to the MCAP file. When the handle is
closed or dropped, the sink will be unregistered from the :class:`foxglove::Context`, and the file will be
finalized and flushed.

To create a live visualization server sink, call :func:`foxglove::WebSocketServer::create`. By default, the server
listens on ``127.0.0.1:8765``. Each client that connects to the websocket server is its own independent
sink. The sink is dynamically added to the :class:`foxglove::Context` associated with the server when the client
connects, and removed from the context when the client disconnects.



.. toctree::
   :maxdepth: 2
   :caption: Contents:

   examples
   generated/api/library_root
