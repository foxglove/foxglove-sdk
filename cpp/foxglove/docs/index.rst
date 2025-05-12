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

Overview
--------

To record messages, you need at least one sink and at least one channel.

A "sink" is a destination for logged messages — either an MCAP file or a live visualization server.
Use :func:`foxglove::McapWriter::create` to create a new MCAP sink. Use
:func:`foxglove::WebSocketServer::create` to create a new live visualization server.

A "channel" gives a way to log related messages which have the same schema. Each channel is
instantiated with a unique topic name.

You can log messages with arbitrary schemas and provide your own encoding, by instantiating a
:class:`foxglove::Channel`.


.. toctree::
   :maxdepth: 2
   :caption: Contents:

   examples
   generated/api/library_root
