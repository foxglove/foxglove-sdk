use std::borrow::Cow;

use bytes::BufMut;
use schemars::{gen::SchemaSettings, JsonSchema};
use serde::Serialize;

use crate::Schema;

/// A trait representing a message that can be logged to a channel.
///
/// Implementing this trait for your type `T` enables the use of [`Channel<T>`][crate::Channel],
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::channel_builder::ChannelBuilder;
    use crate::{Context, Schema};
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
            .build::<TestMessage>();

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
