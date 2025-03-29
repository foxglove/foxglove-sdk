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
//! You can also define your own custom data types by annotating a struct with
//! `#derive(foxglove::Loggable)`. This will automatically implement the
//! [`Encode`] trait, which allows you to log your struct to a channel.
//!
//! ```no_run
//! #[derive(foxglove::Loggable)]
//! struct Custom {
//!     msg: String,
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

pub use bytes;
pub use channel::{Channel, ChannelId, Schema};
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
