//! The official [Foxglove] SDK.
//!
//! This crate provides support for integrating with the Foxglove platform. It can be used to log
//! events to local [MCAP] files or a local visualization server that communicates with the
//! Foxglove app.
//!
//! [Foxglove]: https://docs.foxglove.dev/
//! [MCAP]: https://mcap.dev/
//!
//! # Getting started
//!
//! To record messages, you need at least one sink, and at least one channel. In this example, we
//! create an MCAP file sink, and a channel for [`Log`](`crate::schemas::Log`) messages on a topic
//! called `"/log"`. Then we write one log message and close the file.
//!
//! ```no_run
//! use foxglove::{McapWriter, TypedChannel};
//! use foxglove::schemas::Log;
//!
//! # fn func() -> Result<(), foxglove::FoxgloveError> {
//! let mcap = McapWriter::new().create_new_buffered_file("test.mcap")?;
//!
//! let channel = TypedChannel::new("/log")?;
//! channel.log(&Log{
//!     message: "Hello, Foxglove!".to_string(),
//!     ..Default::default()
//! });
//!
//! mcap.close()?;
//! # Ok(()) }
//! ```
//!
//! # Concepts
//!
//! ## Channels
//!
//! A "channel" gives a way to log related messages which have the same type, or [`Schema`].
//! Each channel is instantiated with a unique "topic", or name, which is typically prefixed by a `/`.
//! If you're familiar with MCAP, it's the same concept as an [MCAP channel]:
//!
//! [MCAP channel]: https://mcap.dev/guides/concepts#channel
//!
//! ### Well-known types
//!
//! The SDK provides [structs for well-known schemas](schemas). These can be used in
//! conjunction with [`TypedChannel`] for type-safe logging, which ensures at compile time that
//! messages logged to a channel all share a common schema.
//!
//! ### Custom data
//!
//! You can also define your own custom data types by implementing the [`Encode`] trait. This
//! allows you to log arbitrary custom data types. Notably, the `Encode` trait is
//! automatically implemented for types that implement [`Serialize`](serde::Serialize) and
//! [`JsonSchema`][jsonschema-trait]. This makes it easy to define new custom messages:
//!
//! ```no_run
//! #[derive(serde::Serialize, schemars::JsonSchema)]
//! struct Custom<'a> {
//!     msg: &'a str,
//!     count: u32,
//! }
//!
//! # fn func() -> Result<(), foxglove::FoxgloveError> {
//! let channel = foxglove::TypedChannel::new("/custom")?;
//! channel.log(&Custom{
//!     msg: "custom",
//!     count: 42
//! });
//! # Ok(()) }
//! ```
//!
//! ### Static Channels
//!
//! A common pattern is to create the channels once as static variables, and then use them
//! throughout the application. To support this, the [`static_typed_channel!`] macro
//! provides a convenient way to create static channels:
//!
//! ```no_run
//! foxglove::static_typed_channel!(pub(crate) BOXES, "/boxes", foxglove::schemas::SceneUpdate);
//! ```
//!
//! [jsonschema-trait]: https://docs.rs/schemars/latest/schemars/trait.JsonSchema.html
//!
//! ## Sinks
//!
//! A "sink" is a destination for logged messages. If you do not configure a sink, log messages
//! will simply be dropped without being recorded. You can configure multiple sinks, and you can
//! create or destroy them dynamically at runtime.
//!
//! ### MCAP file
//!
//! Use [`McapWriter::new()`] to register a new MCAP writer. As long as the handle remains in
//! scope, events will be logged to the MCAP file. When the handle is closed or dropped, the file
//! will be finalized and flushed.
//!
//! ```no_run
//! # fn func() -> Result<(), foxglove::FoxgloveError> {
//! let mcap = foxglove::McapWriter::new()
//!     .create_new_buffered_file("test.mcap")?;
//! # Ok(()) }
//! ```
//!
//! You can override the MCAP writer's configuration using [`McapWriter::with_options`]. See
//! [`WriteOptions`](`mcap::WriteOptions`) for more detail about these parameters:
//!
//! ```no_run
//! # fn func() -> Result<(), foxglove::FoxgloveError> {
//! let options = mcap::WriteOptions::default()
//!     .chunk_size(Some(1024*1024))
//!     .compression(Some(mcap::Compression::Lz4));
//!
//! let mcap = foxglove::McapWriter::with_options(options)
//!     .create_new_buffered_file("test.mcap")?;
//! # Ok(()) }
//! ```
//!
//! ### Live visualization server
//!
//! You can use the SDK to publish messages to the Foxglove app.
//!
//! Use [`WebSocketServer::new`] to create a new live visualization server. By default, the server
//! listens on `127.0.0.1:8765`. Once the server is configured, call [`WebSocketServer::start`] to
//! register the server as a message sink, and begin accepting websocket connections from the
//! Foxglove app.
//!
//! See the ["Connect" documentation][app-connect] for how to connect the Foxglove app to your running
//! server.
//!
//! Note that the server remains running until the process exits, even if the handle is dropped.
//! Use [`stop`](`WebSocketServerHandle::stop`) to shut down the server explicitly.
//!
//! [app-connect]: https://docs.foxglove.dev/docs/connecting-to-data/frameworks/custom#connect
//!
//! ```no_run
//! # async fn func() {
//! let server = foxglove::WebSocketServer::new()
//!     .name("Wall-E")
//!     .bind("127.0.0.1", 9999)
//!     .start()
//!     .await
//!     .expect("Failed to start visualization server");
//!
//! // Log stuff here.
//!
//! server.stop().await;
//! # }
//! ```
//!
//! # Requirements
//!
//! The Foxglove SDK depends on [tokio] as its async runtime with the `rt-multi-thread`
//! feature enabled. Refer to the tokio documentation for more information about how to configure
//! your application to use tokio.
//!
//! [tokio]: https://docs.rs/tokio/latest/tokio/

#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

use thiserror::Error;

mod channel;
mod channel_builder;
mod collection;
mod context;
pub mod convert;
mod cow_vec;
mod encode;
mod log_sink_set;
mod mcap_writer;
mod metadata;
mod runtime;
pub mod schemas;
mod schemas_wkt;
mod sink;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod testutil;
mod time;
pub mod websocket;
mod websocket_server;

pub use channel::{Channel, Schema};
pub use channel_builder::ChannelBuilder;
#[doc(hidden)]
pub use context::Context;
pub use encode::{Encode, TypedChannel};
pub use mcap_writer::{McapWriter, McapWriterHandle};
pub use metadata::{Metadata, PartialMetadata};
pub(crate) use runtime::get_runtime_handle;
pub use runtime::shutdown_runtime;
pub use sink::{Sink, SinkId};
pub(crate) use time::nanoseconds_since_epoch;
pub use websocket_server::{WebSocketServer, WebSocketServerBlockingHandle, WebSocketServerHandle};

/// An error type for errors generated by this crate.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum FoxgloveError {
    /// An unspecified error.
    #[error("{0}")]
    Unspecified(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
    /// The sink dropped a message because it is closed.
    #[error("Sink closed")]
    SinkClosed,
    /// A schema is required.
    #[error("Schema is required")]
    SchemaRequired,
    /// A message encoding is required.
    #[error("Message encoding is required")]
    MessageEncodingRequired,
    /// The server was already started.
    #[error("Server already started")]
    ServerAlreadyStarted,
    /// Failed to bind to the specified host and port.
    #[error("Failed to bind port: {0}")]
    Bind(std::io::Error),
    /// A channel for the same topic has already been registered.
    #[error("Channel for topic {0} already exists in registry")]
    DuplicateChannel(String),
    /// A service with the same name is already registered.
    #[error("Service {0} already exists in registry")]
    DuplicateService(String),
    /// Niether the service nor the server declared supported encodings.
    #[error("Neither service {0} nor the server declared a supported request encoding")]
    MissingRequestEncoding(String),
    /// Services are not supported on this server instance.
    #[error("Services are not supported on this server instance")]
    ServicesNotSupported,
    /// Connection graph is not supported on this server instance.
    #[error("Connection graph is not supported on this server instance")]
    ConnectionGraphNotSupported,
    /// An I/O error.
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    /// An error related to MCAP encoding.
    #[error("MCAP error: {0}")]
    McapError(#[from] mcap::McapError),
}

impl From<convert::RangeError> for FoxgloveError {
    fn from(err: convert::RangeError) -> Self {
        FoxgloveError::Unspecified(err.into())
    }
}
