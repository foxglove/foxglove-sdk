//! Client advertise message types.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::schema::{self, Schema};
use crate::websocket::ws_protocol::JsonMessage;

/// Client advertise message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#client-advertise>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "advertise", rename_all = "camelCase")]
pub struct Advertise<'a> {
    /// Advertised channels.
    #[serde(borrow)]
    pub channels: Vec<Channel<'a>>,
}

impl<'a> Advertise<'a> {
    /// Creates a new advertise message
    pub fn new(channels: impl IntoIterator<Item = Channel<'a>>) -> Self {
        Self {
            channels: channels.into_iter().collect(),
        }
    }

    /// Returns an owned version of this message.
    pub fn into_owned(self) -> Advertise<'static> {
        Advertise {
            channels: self.channels.into_iter().map(|c| c.into_owned()).collect(),
        }
    }
}

impl JsonMessage for Advertise<'_> {}

/// Client channel advertisement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel<'a> {
    /// Channel ID.
    pub id: u32,
    /// Topic name.
    #[serde(borrow)]
    pub topic: Cow<'a, str>,
    /// Message encoding.
    #[serde(borrow)]
    pub encoding: Cow<'a, str>,
    /// Schema name.
    #[serde(borrow)]
    pub schema_name: Cow<'a, str>,
    /// Schema encoding.
    #[serde(borrow, skip_serializing_if = "Option::is_none")]
    pub schema_encoding: Option<Cow<'a, str>>,
    /// Schema data.
    ///
    /// This is the protocol-encoded form. You can use the [`Channel::schema`] method to
    /// decode it.
    #[serde(borrow, skip_serializing_if = "Option::is_none")]
    pub schema: Option<Cow<'a, str>>,
}

impl<'a> Channel<'a> {
    /// Creates a new builder for a channel advertisement.
    #[must_use]
    pub fn builder(
        id: u32,
        topic: impl Into<Cow<'a, str>>,
        encoding: impl Into<Cow<'a, str>>,
    ) -> ChannelBuilder<'a> {
        ChannelBuilder {
            id,
            topic: topic.into(),
            encoding: encoding.into(),
            schema: None,
        }
    }

    /// Returns the decoded schema data.
    pub fn decode_schema(&self) -> Result<Vec<u8>, schema::DecodeError> {
        if let Some((schema_encoding, schema)) =
            self.schema_encoding.as_ref().zip(self.schema.as_ref())
        {
            schema::decode_schema_data(schema_encoding, schema)
        } else {
            Err(schema::DecodeError::MissingSchema)
        }
    }

    /// Returns an owned version of this channel.
    pub fn into_owned(self) -> Channel<'static> {
        Channel {
            id: self.id,
            topic: self.topic.into_owned().into(),
            encoding: self.encoding.into_owned().into(),
            schema_name: self.schema_name.into_owned().into(),
            schema_encoding: self.schema_encoding.map(|s| s.into_owned().into()),
            schema: self.schema.map(|s| s.into_owned().into()),
        }
    }
}

impl<'a> TryFrom<Channel<'a>> for Schema<'a> {
    type Error = schema::DecodeError;

    fn try_from(value: Channel<'a>) -> Result<Self, schema::DecodeError> {
        let schema = value.decode_schema()?;
        Ok(Schema::new(
            value.schema_name,
            value.schema_encoding.unwrap_or_default(),
            schema,
        ))
    }
}

/// Client channel advertisement builder.
pub struct ChannelBuilder<'a> {
    id: u32,
    topic: Cow<'a, str>,
    encoding: Cow<'a, str>,
    schema: Option<Schema<'a>>,
}

impl<'a> ChannelBuilder<'a> {
    /// Adds a schema to the channel advertisement.
    #[must_use]
    pub fn with_schema(mut self, schema: Schema<'a>) -> Self {
        self.schema = Some(schema);
        self
    }

    /// Constructs the channel advertisement.
    pub fn build(self) -> Result<Channel<'a>, schema::EncodeError> {
        match self.schema {
            None => {
                if schema::is_schema_required(&self.encoding) {
                    Err(schema::EncodeError::MissingSchema)
                } else {
                    Ok(Channel {
                        id: self.id,
                        topic: self.topic,
                        encoding: self.encoding,
                        schema_name: "".into(),
                        schema_encoding: None,
                        schema: None,
                    })
                }
            }
            Some(schema) => Ok(Channel {
                id: self.id,
                topic: self.topic,
                encoding: self.encoding,
                schema: Some(schema::encode_schema_data(&schema.encoding, schema.data)?),
                schema_name: schema.name,
                schema_encoding: Some(schema.encoding),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::client::ClientMessage;

    use super::*;

    fn message() -> Advertise<'static> {
        Advertise::new([
            Channel::builder(10, "/t1", "json").build().unwrap(),
            Channel::builder(20, "/t2", "json")
                .with_schema(Schema::new(
                    "t2-schema",
                    "jsonschema",
                    br#"{"type": "object"}"#,
                ))
                .build()
                .unwrap(),
            Channel::builder(30, "/t3", "protobuf")
                .with_schema(Schema::new(
                    "t3-schema",
                    "protobuf",
                    &[0xde, 0xad, 0xbe, 0xef],
                ))
                .build()
                .unwrap(),
        ])
    }

    #[test]
    fn test_encode() {
        insta::assert_json_snapshot!(message());
    }

    #[test]
    fn test_roundtrip() {
        let orig = message();
        let buf = serde_json::to_string(&orig).unwrap();
        let msg = ClientMessage::parse_json(&buf).unwrap();
        assert_eq!(msg, ClientMessage::Advertise(orig));
    }
}
