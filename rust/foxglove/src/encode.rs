use bytes::BufMut;

use crate::Schema;

#[cfg(feature = "schemars")]
mod schemars;

/// A trait representing a message that can be logged to a channel.
///
/// Implementing this trait for your type `T` enables the use of [`Channel<T>`][crate::Channel],
/// which offers a type-checked `log` method.
///
/// # Deriving `Encode`
///
/// This trait may be derived for structs and unit-only enums by enabling the `derive` feature and
/// using the `#[derive(Encode)]` attribute.
///
/// The derive macro is a **convenience** for getting data into Foxglove with minimal friction. It
/// automatically generates a schema and serialization code based on your type's fields. The
/// underlying serialization format is an implementation detail and may change across SDK versions.
///
/// This means the derived schema is **not suitable for schema evolution**. Reordering fields,
/// adding fields in the middle, or removing fields will silently change the tag assignments and
/// break compatibility with previously recorded data. There are no compile-time or runtime
/// warnings when this happens.
///
/// If you need backwards-compatible schema evolution or want your data to be portable and
/// durable across software versions, you should maintain an explicit schema using an established
/// IDL like [protobuf]. You can then implement `Encode` manually for your generated types. See
/// the [SDK documentation](https://docs.foxglove.dev/docs/sdk/logging-messages#logging-with-protobuf-schemas)
/// for an example.
///
/// [protobuf]: https://protobuf.dev/
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
    /// Used as a hint when allocating the buffer for [`Encode::encode`]. For serialization
    /// performance, it's important to provide an accurate estimate, but err on the side of
    /// overestimating. If insufficient buffer space is available based on this estimate,
    /// [`Encode::encode`] will result in an error.
    fn encoded_len(&self) -> Option<usize> {
        None
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::channel_builder::ChannelBuilder;
    use crate::{Context, Schema};
    use serde::Serialize;
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
            .build::<TestMessage>();

        let message = TestMessage {
            msg: "Hello, world!".to_string(),
            count: 42,
        };

        channel.log(&message);
        assert!(!logs_contain("error logging message"));
    }
}
