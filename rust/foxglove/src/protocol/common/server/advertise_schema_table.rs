//! Server advertise message types with a connection-scoped schema table.
//!
//! Schemas are sent at most once per connection: each `advertise` message carries a table of newly
//! introduced schemas, and channels refer to schemas by a connection-local ID rather than inlining
//! the schema data. WebSocket clients receive this form after negotiating `foxglove.sdk.v1.1`.
//! Remote access viewers receive it when their advertised protocol version is 2.3.0 or later.
//! `foxglove.sdk.v2` is not this subprotocol; it is reserved for the remote-access v2 protocol.

use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::protocol::JsonMessage;
use crate::protocol::common::server::advertise::Advertise;
use crate::protocol::schema::{self, DecodeError, EncodeError};

/// Server advertise message with a connection-scoped schema table.
///
/// Sent on WebSocket connections that negotiated `foxglove.sdk.v1.1`, and to remote access
/// viewers at protocol version 2.3.0 or later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "advertise", rename_all = "camelCase")]
pub struct AdvertiseWithSchemaTable<'a> {
    /// Schemas that have not previously been sent on this connection.
    ///
    /// Every schema referenced by [`Channel::schema_id`] is either listed here, or was listed in an
    /// earlier advertise message on the same connection.
    #[serde(borrow, default, skip_serializing_if = "Vec::is_empty")]
    pub schemas: Vec<Schema<'a>>,
    /// Advertised channels.
    #[serde(borrow)]
    pub channels: Vec<Channel<'a>>,
}

impl<'a> AdvertiseWithSchemaTable<'a> {
    /// Creates a new advertise message.
    pub fn new(
        schemas: impl IntoIterator<Item = Schema<'a>>,
        channels: impl IntoIterator<Item = Channel<'a>>,
    ) -> Self {
        Self {
            schemas: schemas.into_iter().collect(),
            channels: channels.into_iter().collect(),
        }
    }

    /// Returns an owned version of this message.
    pub fn into_owned(self) -> AdvertiseWithSchemaTable<'static> {
        AdvertiseWithSchemaTable {
            schemas: self.schemas.into_iter().map(|s| s.into_owned()).collect(),
            channels: self.channels.into_iter().map(|c| c.into_owned()).collect(),
        }
    }
}

impl JsonMessage for AdvertiseWithSchemaTable<'_> {}

/// A schema table entry.
///
/// Schema IDs are allocated by the server, are unique for the lifetime of a connection, and are
/// never reused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema<'a> {
    /// Connection-local schema ID.
    pub id: u64,
    /// Schema name.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// Schema encoding.
    #[serde(borrow)]
    pub encoding: Cow<'a, str>,
    /// Schema data.
    ///
    /// This is the protocol-encoded form. You can use the [`Schema::decode`] method to decode it.
    #[serde(borrow)]
    pub data: Cow<'a, str>,
}

impl<'a> Schema<'a> {
    /// Creates a new schema table entry, encoding the schema data for transport.
    pub fn new(id: u64, schema: schema::Schema<'a>) -> Result<Self, EncodeError> {
        Ok(Self {
            id,
            data: schema::encode_schema_data(&schema.encoding, schema.data)?,
            name: schema.name,
            encoding: schema.encoding,
        })
    }

    /// Returns the decoded schema.
    pub fn decode(&self) -> Result<schema::Schema<'_>, DecodeError> {
        let data = schema::decode_schema_data(&self.encoding, &self.data)?;
        Ok(schema::Schema::new(
            self.name.as_ref(),
            self.encoding.as_ref(),
            data,
        ))
    }

    /// Returns an owned version of this schema.
    pub fn into_owned(self) -> Schema<'static> {
        Schema {
            id: self.id,
            name: self.name.into_owned().into(),
            encoding: self.encoding.into_owned().into(),
            data: self.data.into_owned().into(),
        }
    }
}

/// Server channel advertisement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel<'a> {
    /// Channel ID.
    pub id: u64,
    /// Topic name.
    #[serde(borrow)]
    pub topic: Cow<'a, str>,
    /// Message encoding.
    #[serde(borrow)]
    pub encoding: Cow<'a, str>,
    /// ID of the channel's schema in the connection's schema table.
    ///
    /// `None` for channels that have no schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_id: Option<u64>,
    /// Channel metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl<'a> Channel<'a> {
    /// Creates a new channel advertisement.
    ///
    /// Returns an error if the message encoding requires a schema and none is referenced.
    pub fn new(
        id: u64,
        topic: impl Into<Cow<'a, str>>,
        encoding: impl Into<Cow<'a, str>>,
        schema_id: Option<u64>,
    ) -> Result<Self, EncodeError> {
        let encoding = encoding.into();
        if schema_id.is_none() && schema::is_schema_required(&encoding) {
            return Err(EncodeError::MissingSchema);
        }
        Ok(Self {
            id,
            topic: topic.into(),
            encoding,
            schema_id,
            metadata: BTreeMap::new(),
        })
    }

    /// Adds metadata to the channel advertisement.
    #[must_use]
    pub fn with_metadata(mut self, metadata: BTreeMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Returns an owned version of this channel.
    pub fn into_owned(self) -> Channel<'static> {
        Channel {
            id: self.id,
            topic: self.topic.into_owned().into(),
            encoding: self.encoding.into_owned().into(),
            schema_id: self.schema_id,
            metadata: self.metadata,
        }
    }
}

/// SHA-256 digest of a schema's deduplication key.
type SchemaDigest = [u8; 32];

/// Digests `(name, encoding, protocol-encoded data)` for [`SchemaTable`].
///
/// Each field is length-prefixed so the encoding is unambiguous. The protocol encoding of schema
/// data is deterministic, so digesting the encoded data is equivalent to digesting the raw bytes
/// for a given encoding.
fn schema_digest(name: &str, encoding: &str, data: &str) -> SchemaDigest {
    let mut hasher = Sha256::new();
    for field in [name, encoding, data] {
        let len = u64::try_from(field.len()).expect("schema field length fits in u64");
        hasher.update(len.to_le_bytes());
        hasher.update(field.as_bytes());
    }
    hasher.finalize().into()
}

/// The schemas that have already been sent to one client, keyed for deduplication.
///
/// Schema IDs are connection-local, start at 1, and are never reused: entries are never removed,
/// even when every channel referencing a schema has been unadvertised.
///
/// Entries are keyed by a SHA-256 digest of `(name, encoding, encoded data)` rather than by the
/// schema bytes. The table lives for the whole connection, and the schemas this exists to
/// deduplicate (protobuf descriptor sets) are large, so retaining them would keep a full extra
/// copy per client. A lookup hashes the schema in place and stores 32 bytes. A collision would
/// alias two schemas to one id; SHA-256 makes that negligible for data a connection will see.
#[derive(Debug, Default)]
pub struct SchemaTable {
    ids: HashMap<SchemaDigest, u64>,
}

impl SchemaTable {
    /// Returns the number of schemas in the table.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Returns true if no schemas have been sent yet.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Returns the ID for a schema, allocating a new one if it has not been seen before.
    ///
    /// The boolean is true if the ID was newly allocated.
    fn get_or_insert(&mut self, name: &str, encoding: &str, data: &str) -> (u64, bool) {
        let digest = schema_digest(name, encoding, data);
        let next_id = self.ids.len() as u64 + 1;
        match self.ids.entry(digest) {
            Entry::Occupied(entry) => (*entry.get(), false),
            Entry::Vacant(entry) => (*entry.insert(next_id), true),
        }
    }

    /// Converts an advertisement with inline schemas into the deduplicated form.
    ///
    /// Schemas that have not been sent to this client before are added to the table and listed
    /// in the returned message; channels reference schemas by ID. The caller must enqueue the
    /// returned message before any later message that references the newly allocated IDs.
    pub fn deduplicate<'a>(
        &mut self,
        advertise: &'a Advertise<'_>,
    ) -> AdvertiseWithSchemaTable<'a> {
        let mut schemas = Vec::new();
        let mut channels = Vec::with_capacity(advertise.channels.len());
        for channel in &advertise.channels {
            let has_schema = channel.schema_encoding.is_some()
                || !channel.schema_name.is_empty()
                || !channel.schema.is_empty();
            let schema_id = has_schema.then(|| {
                let encoding = channel.schema_encoding.as_deref().unwrap_or_default();
                let (id, is_new) =
                    self.get_or_insert(&channel.schema_name, encoding, &channel.schema);
                if is_new {
                    schemas.push(Schema {
                        id,
                        name: Cow::Borrowed(&channel.schema_name),
                        encoding: Cow::Borrowed(encoding),
                        data: Cow::Borrowed(&channel.schema),
                    });
                }
                id
            });
            channels.push(Channel {
                id: channel.id,
                topic: Cow::Borrowed(&channel.topic),
                encoding: Cow::Borrowed(&channel.encoding),
                schema_id,
                metadata: channel.metadata.clone(),
            });
        }
        AdvertiseWithSchemaTable { schemas, channels }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    fn message() -> AdvertiseWithSchemaTable<'static> {
        AdvertiseWithSchemaTable::new(
            [
                Schema::new(
                    1,
                    schema::Schema::new("t2-schema", "jsonschema", br#"{"type": "object"}"#),
                )
                .unwrap(),
                Schema::new(
                    2,
                    schema::Schema::new("t3-schema", "protobuf", &[0xde, 0xad, 0xbe, 0xef]),
                )
                .unwrap(),
            ],
            [
                Channel::new(10, "/t1", "json", None).unwrap(),
                Channel::new(20, "/t2", "json", Some(1)).unwrap(),
                Channel::new(30, "/t3", "protobuf", Some(2)).unwrap(),
                Channel::new(40, "/t4", "json", None)
                    .unwrap()
                    .with_metadata(BTreeMap::from([(
                        "foxglove.hasVideoTrack".to_string(),
                        "true".to_string(),
                    )])),
            ],
        )
    }

    #[test]
    fn test_encode() {
        insta::assert_json_snapshot!(message());
    }

    #[test]
    fn test_encode_without_schemas() {
        insta::assert_json_snapshot!(AdvertiseWithSchemaTable::new(
            [],
            [Channel::new(20, "/t2", "json", Some(1)).unwrap()]
        ));
    }

    #[test]
    fn test_roundtrip() {
        let orig = message();
        let buf = orig.to_string();
        let parsed: AdvertiseWithSchemaTable = serde_json::from_str(&buf).unwrap();
        assert_eq!(parsed, orig);
    }

    #[test]
    fn test_parse_without_schemas() {
        let parsed: AdvertiseWithSchemaTable = serde_json::from_str(
            r#"{"op":"advertise","channels":[{"id":20,"topic":"/t2","encoding":"json","schemaId":1}]}"#,
        )
        .unwrap();
        assert!(parsed.schemas.is_empty());
        assert_eq!(parsed.channels[0].schema_id, Some(1));
    }

    #[test]
    fn test_decode_schema() {
        let msg = message();
        let decoded = msg.schemas[1].decode().unwrap();
        assert_eq!(decoded.name, "t3-schema");
        assert_eq!(decoded.encoding, "protobuf");
        assert_eq!(decoded.data.as_ref(), &[0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn test_channel_requires_schema_for_binary_encodings() {
        assert_matches!(
            Channel::new(10, "/t1", "protobuf", None),
            Err(EncodeError::MissingSchema)
        );
        assert!(Channel::new(10, "/t1", "protobuf", Some(1)).is_ok());
    }

    use crate::protocol::common::server::advertise::Channel as InlineChannel;

    fn inline_channel(
        id: u64,
        topic: &'static str,
        schema: Option<schema::Schema<'static>>,
    ) -> InlineChannel<'static> {
        let mut builder = InlineChannel::builder(id, topic, "json");
        if let Some(schema) = schema {
            builder = builder.with_schema(schema);
        }
        builder.build().unwrap()
    }

    fn schema_a() -> schema::Schema<'static> {
        schema::Schema::new("A", "jsonschema", br#"{"type":"object"}"#)
    }

    #[test]
    fn test_schema_table_deduplicates_across_messages() {
        let mut table = SchemaTable::default();

        let msg = table
            .deduplicate(&Advertise::new([
                inline_channel(1, "/a1", Some(schema_a())),
                inline_channel(2, "/a2", Some(schema_a())),
                inline_channel(3, "/none", None),
            ]))
            .into_owned();
        assert_eq!(msg.schemas.len(), 1);
        assert_eq!(msg.schemas[0].id, 1);
        assert_eq!(msg.schemas[0].name, "A");
        assert_eq!(msg.schemas[0].encoding, "jsonschema");
        assert_eq!(msg.schemas[0].data, r#"{"type":"object"}"#);
        let ids: Vec<_> = msg.channels.iter().map(|c| c.schema_id).collect();
        assert_eq!(ids, [Some(1), Some(1), None]);

        // A previously-sent schema is referenced by ID only; a new one gets the next ID.
        let msg = table
            .deduplicate(&Advertise::new([
                inline_channel(2, "/a2", Some(schema_a())),
                inline_channel(4, "/b", Some(schema::Schema::new("B", "jsonschema", b"{}"))),
            ]))
            .into_owned();
        assert_eq!(msg.schemas.len(), 1);
        assert_eq!(msg.schemas[0].id, 2);
        assert_eq!(msg.schemas[0].name, "B");
        let ids: Vec<_> = msg.channels.iter().map(|c| c.schema_id).collect();
        assert_eq!(ids, [Some(1), Some(2)]);

        // Nothing new: the schema table is omitted entirely.
        let msg = table
            .deduplicate(&Advertise::new([inline_channel(
                1,
                "/a1",
                Some(schema_a()),
            )]))
            .into_owned();
        assert!(msg.schemas.is_empty());
        assert_eq!(msg.channels[0].schema_id, Some(1));
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn test_schema_table_distinguishes_name_encoding_and_data() {
        let mut table = SchemaTable::default();
        let msg = table
            .deduplicate(&Advertise::new([
                inline_channel(1, "/1", Some(schema::Schema::new("A", "jsonschema", b"{}"))),
                inline_channel(2, "/2", Some(schema::Schema::new("B", "jsonschema", b"{}"))),
                inline_channel(3, "/3", Some(schema::Schema::new("A", "other", b"{}"))),
                inline_channel(
                    4,
                    "/4",
                    Some(schema::Schema::new("A", "jsonschema", b"{ }")),
                ),
            ]))
            .into_owned();
        assert_eq!(msg.schemas.len(), 4);
        let ids: Vec<_> = msg.channels.iter().map(|c| c.schema_id).collect();
        assert_eq!(ids, [Some(1), Some(2), Some(3), Some(4)]);
    }

    #[test]
    fn test_schema_table_preserves_channel_fields() {
        let mut table = SchemaTable::default();
        let inline = InlineChannel::builder(30, "/t3", "protobuf")
            .with_schema(schema::Schema::new(
                "t3-schema",
                "protobuf",
                &[0xde, 0xad, 0xbe, 0xef],
            ))
            .with_metadata(BTreeMap::from([("k".to_string(), "v".to_string())]))
            .build()
            .unwrap();
        let msg = table.deduplicate(&Advertise::new([inline])).into_owned();
        assert_eq!(msg.schemas[0].data, "3q2+7w==");
        assert_eq!(
            msg.schemas[0].decode().unwrap().data.as_ref(),
            &[0xde, 0xad, 0xbe, 0xef]
        );
        let channel = &msg.channels[0];
        assert_eq!(channel.id, 30);
        assert_eq!(channel.topic, "/t3");
        assert_eq!(channel.encoding, "protobuf");
        assert_eq!(channel.schema_id, Some(1));
        assert_eq!(channel.metadata.get("k").map(String::as_str), Some("v"));
    }
}
