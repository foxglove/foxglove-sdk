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
//! use foxglove::{McapWriter, log};
//! use foxglove::schemas::Log;
//!
//! # fn func() -> Result<(), foxglove::FoxgloveError> {
//! // Create a new MCAP file named 'test.mcap'.
//! let mcap = McapWriter::new().create_new_buffered_file("test.mcap")?;
//!
//! log!("/log", Log{
//!     message: "Hello, Foxglove!".to_string(),
//!     ..Default::default()
//! });
//!
//! // Flush and close the MCAP file.
//! mcap.close()?;
//! # Ok(()) }
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
//! use foxglove::Context;
//! use foxglove::schemas::Log;
//!
//! # fn func() -> Result<(), foxglove::FoxgloveError> {
//! // Create a new context.
//! let ctx = Context::new();
//!
//! // Create a new MCAP file named 'test.mcap'.
//! let mcap = ctx.mcap_writer().create_new_buffered_file("test.mcap")?;
//!
//! // Create a new channel for the topic "/log" for `Log` messages.
//! let channel = ctx.channel_builder("/log").build();
//! channel.log(&Log{
//!     message: "Hello, Foxglove!".to_string(),
//!     ..Default::default()
//! });
//!
//! // Flush and close the MCAP file.
//! mcap.close()?;
//! # Ok(()) }
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
//! use foxglove::{Channel, McapWriter};
//! use foxglove::schemas::Log;
//!
//! # fn func() -> Result<(), foxglove::FoxgloveError> {
//! // Create a new MCAP file named 'test.mcap'.
//! let mcap = McapWriter::new().create_new_buffered_file("test.mcap")?;
//!
//! // Create a new channel for the topic "/log" for `Log` messages.
//! let channel = Channel::new("/log");
//! channel.log(&Log{
//!     message: "Hello, Foxglove!".to_string(),
//!     ..Default::default()
//! });
//!
//! // Flush and close the MCAP file.
//! mcap.close()?;
//! # Ok(()) }
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
//! You can also define your own custom data types by annotating a struct with
//! `#derive(foxglove::Loggable)`. This will automatically implement the [`Encode`] trait, which
//! allows you to log your struct to a channel.
//!
//! ```no_run
//! #[derive(foxglove::Loggable)]
//! struct Custom {
//!     msg: String,
//!     count: u32,
//! }
//!
//! # fn func() -> Result<(), foxglove::FoxgloveError> {
//! let channel = foxglove::Channel::new("/custom");
//! channel.log(&Custom{
//!     msg: "custom",
//!     count: 42
//! });
//! # Ok(()) }
//! ```
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
//! use foxglove::{LazyChannel, LazyRawChannel};
//! use foxglove::schemas::SceneUpdate;
//!
//! static BOXES: LazyChannel<SceneUpdate> = LazyChannel::new("/boxes");
//! static MSG: LazyRawChannel = LazyRawChannel::new("/msg", "json");
//! ```
//!
//! It is also possible to bind lazy channels to an explicit [`LazyContext`]:
//!
//! ```
//! use foxglove::{LazyChannel, LazyContext, LazyRawChannel};
//! use foxglove::schemas::SceneUpdate;
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
//! - `live_visualization`: enables the live visualization server and client, and adds dependencies
//!   on [tokio]. Enabled by default.
//! - `unstable`: features which are under active development and likely to change in an upcoming
//!   version.
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

// Re-export bytes crate for convenience when implementing the `Encode` trait
pub use bytes;
pub use channel::{Channel, ChannelId, LazyChannel, LazyRawChannel, RawChannel};
pub use channel_builder::ChannelBuilder;
pub use context::{Context, LazyContext};
pub use encode::Encode;
pub use mcap_writer::{McapCompression, McapWriteOptions, McapWriter, McapWriterHandle};
pub use metadata::{Metadata, PartialMetadata};
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

extern crate foxglove_derive;
#[doc(hidden)]
pub use foxglove_derive::Loggable;
#[doc(hidden)]
pub use prost_types;

use prost_types::field_descriptor_proto::Type as ProstFieldType;

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

/// Serializes a Protocol Buffers FileDescriptorSet to a byte vector.
///
/// This function encodes the provided FileDescriptorSet message into its binary
/// protobuf representation, which can be used for schema exchange and message
/// type definitions in Foxglove.
///
/// # Arguments
///
/// * `file_descriptor_set` - A reference to the Protocol Buffers FileDescriptorSet to serialize
///
/// # Returns
///
/// A `Vec<u8>` containing the binary protobuf encoding of the FileDescriptorSet
pub fn prost_file_descriptor_set_to_vec(
    file_descriptor_set: &prost_types::FileDescriptorSet,
) -> Vec<u8> {
    use prost::Message;
    file_descriptor_set.encode_to_vec()
}

/// The `ProtobufField` trait defines the interface for types that can be serialized to Protocol
/// Buffer format.
///
/// This trait is automatically implemented for custom types when using the `#[derive(Loggable)]`
/// attribute. It provides the necessary methods to serialize data according to Protocol Buffer
/// encoding rules and generate appropriate Protocol Buffer schema information.
///
/// # Usage
///
/// This trait is typically implemented automatically by using the `#[derive(Loggable)]` attribute
/// on your custom types:
///
/// ```rust
/// #[derive(foxglove::Loggable)]
/// struct MyMessage {
///     number: u64,
///     text: String,
/// }
/// ```
pub trait ProtobufField {
    /// Returns the Protocol Buffer field type that corresponds to this Rust type.
    fn field_type() -> ProstFieldType;

    /// Returns the Protocol Buffer wire type for this Rust type
    fn wire_type() -> u32;

    /// Writes a field with its tag (field number and wire type) to the buffer.
    ///
    /// The default implementation writes the tag followed by the field content.
    fn write_tagged(&self, field_number: u32, buf: &mut impl bytes::BufMut) {
        let tag = (field_number << 3) | Self::wire_type();
        buf.put_u8(tag as u8);
        self.write(buf);
    }

    /// Writes the field content to the output buffer according to Protocol Buffer encoding rules.
    fn write(&self, buf: &mut impl bytes::BufMut);

    /// Returns the type name for the type.
    ///
    /// For complex types (messages, enums) this should return the type name. For primitive types
    /// this should return None (the default).
    fn type_name() -> Option<String> {
        None
    }

    /// If this trait is implemented on an Enum type, this returns the enum descriptor for the type.
    fn enum_descriptor() -> Option<prost_types::EnumDescriptorProto> {
        None
    }

    /// If this trait is implemented on a struct type, this returns the message descriptor for the type.
    fn message_descriptor() -> Option<prost_types::DescriptorProto> {
        None
    }

    /// Indicates the type represents a repeated field (like a Vec).
    ///
    /// By default, fields are not repeated.
    fn repeating() -> bool {
        false
    }
}

// Implement ProtobufField for u64 that serializes the value as a varint in protobuf serialization
impl ProtobufField for u64 {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Uint64
    }

    fn wire_type() -> u32 {
        0
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        let mut value = *self;
        while value >= 0x80 {
            buf.put_u8((value as u8) | 0x80);
            value >>= 7;
        }
        buf.put_u8(value as u8);
    }
}

impl ProtobufField for f32 {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Float
    }

    fn wire_type() -> u32 {
        5 // Float
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        buf.put_f32_le(*self);
    }
}

impl ProtobufField for f64 {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Double
    }

    fn wire_type() -> u32 {
        1 // Double
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        buf.put_f64_le(*self);
    }
}

// Implement ProtobufField for String that serializes the value in protobuf format
impl ProtobufField for String {
    fn field_type() -> ProstFieldType {
        ProstFieldType::String
    }

    fn wire_type() -> u32 {
        2 // Length-delimited
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        // Get the UTF-8 bytes of the string
        let bytes = self.as_bytes();

        // Write the length as a varint
        let len = bytes.len();
        let mut len_value = len as u64;
        while len_value >= 0x80 {
            buf.put_u8((len_value as u8) | 0x80);
            len_value >>= 7;
        }
        buf.put_u8(len_value as u8);

        // Write the string bytes
        buf.put_slice(bytes);
    }
}

// Implement ProtobufField for &str, which delegates to String's implementation
impl ProtobufField for &str {
    fn field_type() -> ProstFieldType {
        <String as ProtobufField>::field_type()
    }

    fn wire_type() -> u32 {
        <String as ProtobufField>::wire_type()
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        // Get the UTF-8 bytes of the string
        let bytes = self.as_bytes();

        // Write the length as a varint
        let len = bytes.len();
        let mut len_value = len as u64;
        while len_value >= 0x80 {
            buf.put_u8((len_value as u8) | 0x80);
            len_value >>= 7;
        }
        buf.put_u8(len_value as u8);

        // Write the string bytes
        buf.put_slice(bytes);
    }
}

// Implement ProtobufField for u32 that serializes the value as a varint in protobuf serialization
impl ProtobufField for u32 {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Uint32
    }

    fn wire_type() -> u32 {
        0 // Varint
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        // For u32, we encode as a varint in the same way as u64
        let mut value = *self as u64;
        while value >= 0x80 {
            buf.put_u8((value as u8) | 0x80);
            value >>= 7;
        }
        buf.put_u8(value as u8);
    }
}

// Implement ProtobufField for u16 that serializes the value as a varint in protobuf serialization
impl ProtobufField for u16 {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Uint32
    }

    fn wire_type() -> u32 {
        0 // Varint
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        // For u16, we encode as a varint in the same way as u32
        let mut value = *self as u64;
        while value >= 0x80 {
            buf.put_u8((value as u8) | 0x80);
            value >>= 7;
        }
        buf.put_u8(value as u8);
    }
}

// implement a protobuf field for any Vec<T> where T implements ProtobufField
impl<T> ProtobufField for Vec<T>
where
    T: ProtobufField,
{
    fn field_type() -> ProstFieldType {
        T::field_type()
    }

    fn wire_type() -> u32 {
        2 // Length-delimited
    }

    fn write_tagged(&self, field_number: u32, buf: &mut impl bytes::BufMut) {
        // non-packed repeated fields are encoded as a record for each element
        // https://protobuf.dev/programming-guides/encoding/#optional
        for value in self {
            let wire_type = T::wire_type();

            let tag = (field_number << 3) | wire_type;
            buf.put_u8(tag as u8);

            value.write(buf);
        }
    }

    fn write(&self, _buf: &mut impl bytes::BufMut) {
        panic!("Vec<T> should always be written using write_tagged");
    }

    fn repeating() -> bool {
        true
    }

    fn message_descriptor() -> Option<prost_types::DescriptorProto> {
        // The message descriptor of a vector is the message descriptor of the element type
        // the "repeating" property is set on the field that is repeating rather than the message
        // descriptor
        T::message_descriptor()
    }

    fn type_name() -> Option<String> {
        T::type_name()
    }
}
