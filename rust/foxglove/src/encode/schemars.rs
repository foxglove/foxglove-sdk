use std::borrow::Cow;

use bytes::BufMut;
use schemars::generate::SchemaSettings;
use schemars::JsonSchema;
use serde::Serialize;

use crate::{Encode, Schema};

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
    use schemars::JsonSchema;
    use serde::Serialize;
    use serde_json::{json, Value};

    use crate::Encode;

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
