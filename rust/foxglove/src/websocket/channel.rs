use super::ws_protocol::{client, schema, server};
use crate::{RawChannel, Schema};

/// A client channel ID.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ClientChannelId(u32);

impl ClientChannelId {
    /// Creates a new client channel ID.
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

impl From<ClientChannelId> for u32 {
    fn from(id: ClientChannelId) -> u32 {
        id.0
    }
}

impl From<ClientChannelId> for u64 {
    fn from(id: ClientChannelId) -> u64 {
        id.0.into()
    }
}

impl std::fmt::Display for ClientChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Information about a channel advertised by the client
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientChannel {
    /// An identifier for this channel, assigned by the client
    pub id: ClientChannelId,
    /// The topic name for this channel
    pub topic: String,
    /// The encoding of messages on this channel
    pub encoding: String,
    /// The name of the schema for this channel
    pub schema_name: String,
    /// The encoding of the schema data
    pub schema_encoding: Option<String>,
    /// May or may not be a UTF-8 string depending on the schema_encoding.
    pub schema: Option<Vec<u8>>,
}

impl TryFrom<client::advertise::Channel<'_>> for ClientChannel {
    type Error = schema::DecodeError;

    fn try_from(ch: client::advertise::Channel) -> Result<Self, Self::Error> {
        let schema = match ch.decode_schema() {
            Ok(schema) => Some(schema),
            Err(schema::DecodeError::MissingSchema) => None,
            Err(e) => return Err(e),
        };
        Ok(Self {
            id: ClientChannelId::new(ch.id),
            topic: ch.topic.to_string(),
            encoding: ch.encoding.to_string(),
            schema_name: ch.schema_name.to_string(),
            schema_encoding: ch.schema_encoding.map(|s| s.to_string()),
            schema,
        })
    }
}

impl<'a> TryFrom<&'a RawChannel> for server::advertise::Channel<'a> {
    type Error = schema::EncodeError;

    fn try_from(ch: &'a RawChannel) -> Result<Self, Self::Error> {
        let mut builder = Self::builder(ch.id().into(), ch.topic(), ch.message_encoding());
        if let Some(schema) = ch.schema() {
            builder = builder.with_schema(schema.into());
        }
        builder.build()
    }
}

impl<'a> From<&'a Schema> for schema::Schema<'a> {
    fn from(schema: &'a Schema) -> Self {
        Self::new(&schema.name, &schema.encoding, schema.data.clone())
    }
}
