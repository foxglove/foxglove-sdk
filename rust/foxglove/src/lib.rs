//! The official [Foxglove] SDK.
//!
//! This crate provides support for integrating with the Foxglove platform. It can be used to log
//! events to local [MCAP] files or a local visualization server that communicates with the Foxglove
//! app.
//!
//! [Foxglove]: https://docs.foxglove.dev/
//! [MCAP]: https://mcap.dev/
//!
//! # Getting started
//!
//! To record messages, you need at least one sink. In this example, we create an MCAP file sink,
//! and log a [`Log`](`crate::schemas::Log`) message on a topic called `/log`. We write one log
//! message and close the file.
//!
//! ```no_run
//! use foxglove::schemas::Log;
//! use foxglove::{log, McapWriter};
//!
//! // Create a new MCAP file named 'test.mcap'.
//! let mcap = McapWriter::new()
//!     .create_new_buffered_file("test.mcap")
//!     .expect("create failed");
//!
//! log!(
//!     "/log",
//!     Log {
//!         message: "Hello, Foxglove!".to_string(),
//!         ..Default::default()
//!     }
//! );
//!
//! // Flush and close the MCAP file.
//! mcap.close().expect("close failed");
//! ```
//!
//! # Concepts
//!
//! ## Context
//!
//! A [`Context`] is the binding between channels and sinks. Each channel and sink belongs to
//! exactly one context. Sinks receive advertisements about channels on the context, and can
//! optionally subscribe to receive logged messages on those channels.
//!
//! When the context goes out of scope, its corresponding channels and sinks will be disconnected
//! from one another, and logging will stop. Attempts to log further messages on the channels will
//! elicit throttled warning messages.
//!
//! Since many applications only need a single context, the SDK provides a static default context
//! for convenience. This default context is the one used in the [example above](#getting-started).
//! If we wanted to use an explicit context instead, we'd write:
//!
//! ```no_run
//! use foxglove::schemas::Log;
//! use foxglove::Context;
//!
//! // Create a new context.
//! let ctx = Context::new();
//!
//! // Create a new MCAP file named 'test.mcap'.
//! let mcap = ctx
//!     .mcap_writer()
//!     .create_new_buffered_file("test.mcap")
//!     .expect("create failed");
//!
//! // Create a new channel for the topic "/log" for `Log` messages.
//! let channel = ctx.channel_builder("/log").build();
//! channel.log(&Log {
//!     message: "Hello, Foxglove!".to_string(),
//!     ..Default::default()
//! });
//!
//! // Flush and close the MCAP file.
//! mcap.close().expect("close failed");
//! ```
//!
//! ## Channels
//!
//! A [`Channel`] gives a way to log related messages which have the same type, or [`Schema`]. Each
//! channel is instantiated with a unique "topic", or name, which is typically prefixed by a `/`. If
//! you're familiar with MCAP, it's the same concept as an [MCAP channel].
//!
//! A channel is always associated with exactly one [`Context`] throughout its lifecycle. The
//! channel remains attached to the context until it is either explicitly closed with
//! [`Channel::close`], or the context is dropped. Attempting to log a message on a closed channel
//! will elicit a throttled warning.
//!
//! [MCAP channel]: https://mcap.dev/guides/concepts#channel
//!
//! In the [example above](#getting-started), `log!` creates a `Channel<Log>` behind the scenes on
//! the first call. The example could be equivalently written as:
//!
//! ```no_run
//! use foxglove::schemas::Log;
//! use foxglove::{Channel, McapWriter};
//!
//! // Create a new MCAP file named 'test.mcap'.
//! let mcap = McapWriter::new()
//!     .create_new_buffered_file("test.mcap")
//!     .expect("create failed");
//!
//! // Create a new channel for the topic "/log" for `Log` messages.
//! let channel = Channel::new("/log");
//! channel.log(&Log {
//!     message: "Hello, Foxglove!".to_string(),
//!     ..Default::default()
//! });
//!
//! // Flush and close the MCAP file.
//! mcap.close().expect("close failed");
//! ```
//!
//! `log!` can be mixed and matched with manually created channels in the default [`Context`], as
//! long as the types are exactly the same.
//!
//! ### Well-known types
//!
//! The SDK provides [structs for well-known schemas](schemas). These can be used in conjunction
//! with [`Channel`] for type-safe logging, which ensures at compile time that messages logged to a
//! channel all share a common schema.
//!
//! ### Custom data
//!
//! You can also define your own custom data types by implementing the [`Encode`] trait.
//!
//! The easiest way to do this is to enable the `derive` feature and derive the [`Encode`] trait,
//! which will generate a schema and allow you to log your struct to a channel. This currently uses
//! protobuf encoding.
//!
//! ```no_run
//! # #[cfg(feature = "derive")]
//! # {
//! #[derive(foxglove::Encode)]
//! struct Custom<'a> {
//!     msg: &'a str,
//!     count: u32,
//! }
//!
//! let channel = foxglove::Channel::new("/custom");
//! channel.log(&Custom {
//!     msg: "custom",
//!     count: 42,
//! });
//! # }
//! ```
//!
//! If you'd like to use JSON encoding for integration with particular tooling, you can enable the
//! `schemars` feature, which will provide a blanket [`Encode`] implementation for types that
//! implement [`Serialize`](serde::Serialize) and [`JsonSchema`][jsonschema-trait].
//!
//! [jsonschema-trait]: https://docs.rs/schemars/latest/schemars/trait.JsonSchema.html
//!
//! ### Lazy Channels
//!
//! A common pattern is to create the channels once as static variables, and then use them
//! throughout the application. But because channels do not have a const initializer, they must be
//! initialized lazily. [`LazyChannel`] and [`LazyRawChannel`] provide a convenient way to do this.
//!
//! Be careful when using this pattern. The channel will not be advertised to sinks until it is
//! initialized, which is guaranteed to happen when the channel is first used. If you need to ensure
//! the channel is initialized _before_ using it, you can use [`LazyChannel::init`].
//!
//! In this example, we create two lazy channels on the default context:
//!
//! ```
//! use foxglove::schemas::SceneUpdate;
//! use foxglove::{LazyChannel, LazyRawChannel};
//!
//! static BOXES: LazyChannel<SceneUpdate> = LazyChannel::new("/boxes");
//! static MSG: LazyRawChannel = LazyRawChannel::new("/msg", "json");
//! ```
//!
//! It is also possible to bind lazy channels to an explicit [`LazyContext`]:
//!
//! ```
//! use foxglove::schemas::SceneUpdate;
//! use foxglove::{LazyChannel, LazyContext, LazyRawChannel};
//!
//! static CTX: LazyContext = LazyContext::new();
//! static BOXES: LazyChannel<SceneUpdate> = CTX.channel("/boxes");
//! static MSG: LazyRawChannel = CTX.raw_channel("/msg", "json");
//! ```
//!
//! ## Sinks
//!
//! A "sink" is a destination for logged messages. If you do not configure a sink, log messages will
//! simply be dropped without being recorded. You can configure multiple sinks, and you can create
//! or destroy them dynamically at runtime.
//!
//! A sink is typically associated with exactly one [`Context`] throughout its lifecycle. Details
//! about the how the sink is registered and unregistered from the context are sink-specific.
//!
//! ### MCAP file
//!
//! Use [`McapWriter::new()`] to register a new MCAP writer. As long as the handle remains in scope,
//! events will be logged to the MCAP file. When the handle is closed or dropped, the sink will be
//! unregistered from the [`Context`], and the file will be finalized and flushed.
//!
//! ```no_run
//! let mcap = foxglove::McapWriter::new()
//!     .create_new_buffered_file("test.mcap")
//!     .expect("create failed");
//! ```
//!
//! You can override the MCAP writer's configuration using [`McapWriter::with_options`]. See
//! [`WriteOptions`](`mcap::WriteOptions`) for more detail about these parameters:
//!
//! ```no_run
//! # #[cfg(feature = "lz4")]
//! # {
//! let options = mcap::WriteOptions::default()
//!     .chunk_size(Some(1024 * 1024))
//!     .compression(Some(mcap::Compression::Lz4));
//!
//! let mcap = foxglove::McapWriter::with_options(options)
//!     .create_new_buffered_file("test.mcap")
//!     .expect("create failed");
//! # }
//! ```
//!
//! ### Live visualization server
//!
//! You can use the SDK to publish messages to the Foxglove app.
//!
//! Note: this requires the `live_visualization` feature, which is enabled by default.
//!
//! Use [`WebSocketServer::new`] to create a new live visualization server. By default, the server
//! listens on `127.0.0.1:8765`. Once the server is configured, call [`WebSocketServer::start`] to
//! start the server, and begin accepting websocket connections from the Foxglove app.
//!
//! Each client that connects to the websocket server is its own independent sink. The sink is
//! dynamically added to the [`Context`] associated with the server when the client connects, and
//! removed from the context when the client disconnects.
//!
//! See the ["Connect" documentation][app-connect] for how to connect the Foxglove app to your
//! running server.
//!
//! Note that the server remains running until the process exits, even if the handle is dropped. Use
//! [`stop`](`WebSocketServerHandle::stop`) to shut down the server explicitly.
//!
//! [app-connect]: https://docs.foxglove.dev/docs/connecting-to-data/frameworks/custom#connect
//!
//! ```no_run
//! # #[cfg(feature = "live_visualization")]
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
//! server.stop();
//! # }
//! ```
//!
//! # Feature flags
//!
//! The Foxglove SDK defines the following feature flags:
//!
//! - `chrono`: enables [chrono] conversions for [`Duration`][crate::schemas::Duration] and
//!   [`Timestamp`][crate::schemas::Timestamp].
//! - `derive`: enables the use of `#[derive(Encode)]` to derive the [`Encode`] trait for logging
//!   custom structs. Enabled by default.
//! - `live_visualization`: enables the live visualization server and client, and adds dependencies
//!   on [tokio]. Enabled by default.
//! - `lz4`: enables support for the LZ4 compression algorithm for mcap files. Enabled by default.
//! - `schemars`: provides a blanket implementation of the [`Encode`] trait for types that
//!   implement [`Serialize`](serde::Serialize) and [`JsonSchema`][jsonschema-trait].
//! - `unstable`: features which are under active development and likely to change in an upcoming
//!   version.
//! - `zstd`: enables support for the zstd compression algorithm for mcap files. Enabled by
//!   default.
//!
//! If you do not require live visualization features, you can disable that flag to reduce the
//! compiled size of the SDK.
//!
//! # Requirements
//!
//! With the `live_visualization` feature (enabled by default), the Foxglove SDK depends on [tokio]
//! as its async runtime. See [`WebSocketServer`] for more information. Refer to the tokio
//! documentation for more information about how to configure your application to use tokio.
//!
//! [chrono]: https://docs.rs/chrono/latest/chrono/
//! [tokio]: https://docs.rs/tokio/latest/tokio/

#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

use thiserror::Error;

mod app_url;
mod channel;
mod channel_builder;
mod context;
pub mod convert;
mod encode;
pub mod library_version;
#[doc(hidden)]
pub mod log_macro;
mod log_sink_set;
mod mcap_writer;
mod metadata;
#[doc(hidden)]
#[cfg(feature = "derive")]
pub mod protobuf;
mod schema;
pub mod schemas;
mod schemas_wkt;
mod sink;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod testutil;
mod throttler;
mod time;

pub use app_url::AppUrl;
// Re-export bytes crate for convenience when implementing the `Encode` trait
pub use bytes;
pub use channel::{Channel, ChannelId, LazyChannel, LazyRawChannel, RawChannel};
pub use channel_builder::ChannelBuilder;
pub use context::{Context, LazyContext};
pub use encode::Encode;
pub use mcap_writer::{McapCompression, McapWriteOptions, McapWriter, McapWriterHandle};
pub use metadata::{Metadata, PartialMetadata, ToUnixNanos};
pub use schema::Schema;
pub use sink::{Sink, SinkId};
pub(crate) use time::nanoseconds_since_epoch;

#[cfg(feature = "live_visualization")]
mod runtime;
#[cfg(feature = "live_visualization")]
pub mod websocket;
#[cfg(feature = "live_visualization")]
mod websocket_server;
#[cfg(feature = "live_visualization")]
pub(crate) use runtime::get_runtime_handle;
#[cfg(feature = "live_visualization")]
pub use runtime::shutdown_runtime;
#[cfg(feature = "live_visualization")]
pub use websocket_server::{WebSocketServer, WebSocketServerHandle};

#[doc(hidden)]
#[cfg(feature = "derive")]
pub use foxglove_derive::Encode;
#[doc(hidden)]
#[cfg(feature = "derive")]
pub use prost_types;

/// An error type for errors generated by this crate.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum FoxgloveError {
    /// An unspecified error.
    #[error("{0}")]
    Unspecified(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
    /// A value or argument is invalid.
    #[error("Value or argument is invalid: {0}")]
    ValueError(String),
    /// A UTF-8 error.
    #[error("{0}")]
    Utf8Error(String),
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
    /// A service with the same name is already registered.
    #[error("Service {0} has already been registered")]
    DuplicateService(String),
    /// Neither the service nor the server declared supported encodings.
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
    /// An error related to MCAP writing.
    #[error("MCAP error: {0}")]
    McapError(#[from] mcap::McapError),
    /// An error occurred while encoding a message.
    #[error("Encoding error: {0}")]
    EncodeError(String),
}

impl From<convert::RangeError> for FoxgloveError {
    fn from(err: convert::RangeError) -> Self {
        FoxgloveError::ValueError(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for FoxgloveError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        FoxgloveError::Utf8Error(err.to_string())
    }
}

impl From<std::str::Utf8Error> for FoxgloveError {
    fn from(err: std::str::Utf8Error) -> Self {
        FoxgloveError::Utf8Error(err.to_string())
    }
}
