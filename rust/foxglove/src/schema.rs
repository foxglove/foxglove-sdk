use std::borrow::Cow;

/// A Schema is a description of the data format of messages in a channel.
///
/// It allows Foxglove to validate messages and provide richer visualizations.
/// You can use the well known types provided in the [crate::schemas] module or provide your own.
/// See the [MCAP spec](https://mcap.dev/spec#schema-op0x03) for more information.
#[derive(Clone, PartialEq, Eq)]
pub struct Schema {
    /// An identifier for the schema.
    pub name: String,
    /// The encoding of the schema data. For example "jsonschema" or "protobuf".
    /// The [well-known schema encodings](https://mcap.dev/spec/registry#well-known-schema-encodings) are preferred.
    pub encoding: String,
    /// Must conform to the schema encoding. If encoding is an empty string, data should be 0 length.
    pub data: Cow<'static, [u8]>,
}

impl std::fmt::Debug for Schema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Schema")
            .field("name", &self.name)
            .field("encoding", &self.encoding)
            .finish_non_exhaustive()
    }
}

impl Schema {
    /// Returns a new schema.
    pub fn new(
        name: impl Into<String>,
        encoding: impl Into<String>,
        data: impl Into<Cow<'static, [u8]>>,
    ) -> Self {
        Self {
            name: name.into(),
            encoding: encoding.into(),
            data: data.into(),
        }
    }

    /// Returns a JSON schema for the specified type.
    pub fn json_schema<T: schemars::JsonSchema>() -> Self {
        let json_schema = schemars::schema_for!(T);
        Self::new(
            std::any::type_name::<T>(),
            "jsonschema",
            serde_json::to_vec(&json_schema).expect("Failed to serialize schema"),
        )
    }
}
