use std::{borrow::Cow, collections::BTreeMap, sync::Arc};

use bytes::BufMut;
use delegate::delegate;
use schemars::{gen::SchemaSettings, JsonSchema};
use serde::Serialize;

use crate::{channel::ChannelId, Channel, ChannelBuilder, FoxgloveError, PartialMetadata, Schema};

const STACK_BUFFER_SIZE: usize = 128 * 1024;

/// A trait representing a message that can be logged to a [`Channel`].
///
/// Implementing this trait for your type `T` enables the use of [`TypedChannel<T>`],
/// which offers a type-checked `log` method.
pub trait Encode {
    /// The error type returned by methods in this trait.
    type Error: std::error::Error;

    /// Returns the schema for your data.
    ///
    /// You may return `None` for rare situations where the schema is not known. Note that
    /// downstream consumers of the recording may not be able to interpret your data as a result.
    fn get_schema() -> Option<Schema>;

    /// Returns the message encoding for your data.
    ///
    /// Typically one of "protobuf" or "json".
    fn get_message_encoding() -> String;

    /// Encodes message data to the provided buffer.
    fn encode(&self, buf: &mut impl BufMut) -> Result<(), Self::Error>;

    /// Optional. Returns an estimated encoded length for the message data.
    ///
    /// Used as a hint when allocating the buffer for [`Encode::encode`].
    fn encoded_len(&self) -> Option<usize> {
        None
    }
}

/// Automatically implements [`Encode`] for any type that implements [`Serialize`] and
/// [`JsonSchema`](https://docs.rs/schemars/latest/schemars/trait.JsonSchema.html). See the
/// JsonSchema Trait and SchemaGenerator from the [schemars
/// crate](https://docs.rs/schemars/latest/schemars/) for more information.
/// Definitions are inlined since Foxglove does not support external references.
impl<T: Serialize + JsonSchema> Encode for T {
    type Error = serde_json::Error;

    fn get_schema() -> Option<Schema> {
        let settings = SchemaSettings::draft07().with(|option| {
            option.inline_subschemas = true;
        });
        let generator = settings.into_generator();
        let json_schema = generator.into_root_schema_for::<T>();

        Some(Schema::new(
            std::any::type_name::<T>().to_string(),
            "jsonschema".to_string(),
            Cow::Owned(serde_json::to_vec(&json_schema).expect("Failed to serialize schema")),
        ))
    }

    fn get_message_encoding() -> String {
        "json".to_string()
    }

    fn encode(&self, buf: &mut impl BufMut) -> Result<(), Self::Error> {
        serde_json::to_writer(buf.writer(), self)
    }
}

/// A typed [`Channel`] for messages that implement [`Encode`].
///
/// Channels are immutable, returned as `Arc<Channel>` and can be shared between threads.
#[derive(Debug)]
pub struct TypedChannel<T: Encode> {
    inner: Arc<Channel>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Encode> TypedChannel<T> {
    /// Constructs a new typed channel with default settings.
    ///
    /// If you want to override the channel configuration, use [`ChannelBuilder::build_typed`].
    pub fn new(topic: impl Into<String>) -> Result<Self, FoxgloveError> {
        ChannelBuilder::new(topic).build_typed()
    }

    pub(crate) fn from_channel(channel: Arc<Channel>) -> Self {
        Self {
            inner: channel,
            _phantom: std::marker::PhantomData,
        }
    }

    delegate! { to self.inner {
        /// Returns the channel ID.
        pub fn id(&self) -> ChannelId;

        /// Returns the topic name of the channel.
        pub fn topic(&self) -> &str;

        /// Returns the channel schema.
        pub fn schema(&self) -> Option<&Schema>;

        /// Returns the message encoding for this channel.
        pub fn message_encoding(&self) -> &str;

        /// Returns the metadata for this channel.
        pub fn metadata(&self) -> &BTreeMap<String, String>;

        /// Returns true if there's at least one sink subscribed to this channel.
        pub fn has_sinks(&self) -> bool;
    } }

    /// Encodes the message and logs it on the channel.
    pub fn log(&self, msg: &T) {
        if self.has_sinks() {
            self.log_to_sinks(msg, PartialMetadata::default());
        }
    }

    /// Encodes the message and logs it on the channel with additional metadata.
    pub fn log_with_meta(&self, msg: &T, metadata: PartialMetadata) {
        if self.has_sinks() {
            self.log_to_sinks(msg, metadata);
        }
    }

    fn log_to_sinks(&self, msg: &T, metadata: PartialMetadata) {
        // Try to avoid heap allocation by using a stack buffer.
        let mut stack_buf = [0u8; STACK_BUFFER_SIZE];
        let mut cursor = &mut stack_buf[..];

        match msg.encode(&mut cursor) {
            Ok(()) => {
                // Compute the written amount of bytes
                let written = cursor.as_ptr() as usize - stack_buf.as_ptr() as usize;
                self.inner.log_to_sinks(&stack_buf[..written], metadata);
            }
            Err(_) => {
                // Likely the stack buffer was too small, so fall back to a heap buffer.
                let mut size = msg.encoded_len().unwrap_or(STACK_BUFFER_SIZE * 2);
                if size <= STACK_BUFFER_SIZE {
                    // The estimate in `encoded_len` was too small, fall back to stack buffer size * 2
                    size = STACK_BUFFER_SIZE * 2;
                }
                let mut buf = Vec::with_capacity(size);
                if let Err(err) = msg.encode(&mut buf) {
                    tracing::error!("failed to encode message: {:?}", err);
                }
                self.inner.log_to_sinks(&buf, metadata);
            }
        }
    }
}

/// Registers a static [`TypedChannel`] for the provided topic and message type.
///
/// This macro is a wrapper around [`LazyLock<TypedChannel<T>>`](std::sync::LazyLock),
/// which initializes the channel lazily upon first use. If the initialization fails (e.g., due to
/// [`FoxgloveError::DuplicateChannel`]), the program will panic.
///
/// If you don't require a static variable, you can just use [`TypedChannel::new()`] directly.
///
/// The channel is created with the provided visibility and identifier, and the topic and message type.
///
/// # Example
/// ```
/// use foxglove::static_typed_channel;
/// use foxglove::schemas::{FrameTransform, SceneUpdate};
///
/// // A locally-scoped typed channel.
/// static_typed_channel!(TF, "/tf", FrameTransform);
///
/// // A pub(crate)-scoped typed channel.
/// static_typed_channel!(pub(crate) BOXES, "/boxes", SceneUpdate);
///
/// // Usage (you would populate the structs, rather than using `default()`).
/// TF.log(&FrameTransform::default());
/// BOXES.log(&SceneUpdate::default());
/// ```
#[macro_export]
macro_rules! static_typed_channel {
    ($vis:vis $ident: ident, $topic: literal, $ty: ty) => {
        $vis static $ident: std::sync::LazyLock<$crate::TypedChannel<$ty>> =
            std::sync::LazyLock::new(|| match $crate::TypedChannel::new($topic) {
                Ok(channel) => channel,
                Err(e) => {
                    panic!("Failed to create channel for {}: {:?}", $topic, e);
                }
            });
    };
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::channel_builder::ChannelBuilder;
    use crate::{Context, Schema};
    use prost::bytes::BufMut;
    use serde::Serialize;
    use serde_json::{json, Value};
    use tracing_test::traced_test;

    #[derive(Debug, Serialize)]
    struct TestMessage {
        msg: String,
        count: u32,
    }

    impl Encode for TestMessage {
        type Error = serde_json::Error;

        fn get_schema() -> Option<Schema> {
            Some(Schema::new(
                "TextMessage",
                "jsonschema",
                br#"{
                    "type": "object",
                    "properties": {
                        "msg": {"type": "string"},
                        "count": {"type": "number"},
                    },
                }"#,
            ))
        }

        fn get_message_encoding() -> String {
            "json".to_string()
        }

        fn encode(&self, buf: &mut impl BufMut) -> Result<(), Self::Error> {
            serde_json::to_writer(buf.writer(), self)
        }
    }

    #[traced_test]
    #[test]
    fn test_json_typed_channel() {
        let ctx = Context::new();
        let channel = ChannelBuilder::new("topic2")
            .context(&ctx)
            .build_typed::<TestMessage>()
            .expect("failed to build channel");

        let message = TestMessage {
            msg: "Hello, world!".to_string(),
            count: 42,
        };

        channel.log(&message);
        assert!(!logs_contain("error logging message"));
    }

    #[test]
    fn test_derived_schema_inlines_enums() {
        #[derive(Serialize, JsonSchema)]
        #[allow(dead_code)]
        enum Foo {
            A,
        }

        #[derive(Serialize, JsonSchema)]
        struct Bar {
            foo: Foo,
        }

        let schema = Bar::get_schema();
        assert!(schema.is_some());

        let schema = schema.unwrap();
        assert_eq!(schema.encoding, "jsonschema");

        let json: Value = serde_json::from_slice(&schema.data).expect("failed to parse schema");
        assert_eq!(json["properties"]["foo"]["enum"], json!(["A"]));
    }
}
