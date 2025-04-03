# Foxglove

The official [Foxglove] SDK.

This crate provides support for integrating with the Foxglove platform. It can be used to log
events to local [MCAP] files or a local visualization server that communicates with the
Foxglove app.

[Foxglove]: https://docs.foxglove.dev/
[MCAP]: https://mcap.dev/

# Overview

To record messages, you need at least one sink and at least one channel.

A "sink" is a destination for logged messages — either an MCAP file or a live visualization server. Use `McapWriter::new()` to register a new MCAP sink. Use `WebSocketServer::new` to create a new live visualization server.

A "channel" gives a way to log related messages which have the same schema. Each channel is instantiated with a unique topic name.

The SDK provides structs for well-known schemas. These can be used in conjunction with
`Channel` for type-safe logging, which ensures at compile time that
messages logged to a channel all share a common schema.

You can also define your own custom data types by implementing the `Encode` trait. This
allows you to log arbitrary custom data types. Notably, the `Encode` trait is
automatically implemented for types that implement `serde::Serialize` and
`schemars::JsonSchema`. This makes it easy to define new custom messages.

# Get Started

For more information and examples, see [docs.rs](https://docs.rs/foxglove).

# Supported Rust Versions

The current MSRV (minimum supported rust version) is 1.83.0.
