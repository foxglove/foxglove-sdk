//! Advertise services message types.

use std::borrow::Cow;
use std::convert::TryFrom;

use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::schema::{self, Schema};
use crate::websocket::ws_protocol::JsonMessage;

/// Advertise services message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#advertise-services>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "advertiseServices", rename_all = "camelCase")]
pub struct AdvertiseServices<'a> {
    /// Services.
    #[serde(borrow)]
    pub services: Vec<Service<'a>>,
}

impl<'a> AdvertiseServices<'a> {
    /// Creates a new advertise services message.
    pub fn new(services: impl IntoIterator<Item = Service<'a>>) -> Self {
        Self {
            services: services.into_iter().collect(),
        }
    }

    /// Returns an owned version of this message.
    pub fn into_owned(self) -> AdvertiseServices<'static> {
        AdvertiseServices {
            services: self.services.into_iter().map(|s| s.into_owned()).collect(),
        }
    }
}

impl JsonMessage for AdvertiseServices<'_> {}

/// A service in a [`AdvertiseServices`] message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service<'a> {
    /// Service ID.
    pub id: u32,
    /// Service name.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// Service type, which may be used to derive the request & response schema.
    #[serde(borrow)]
    pub r#type: Cow<'a, str>,
    /// Request schema.
    #[serde(borrow, skip_serializing_if = "Option::is_none")]
    pub request: Option<MessageSchema<'a>>,
    /// Request schema name.
    #[serde(borrow, skip_serializing_if = "Option::is_none")]
    pub request_schema: Option<Cow<'a, str>>,
    /// Response schema.
    #[serde(borrow, skip_serializing_if = "Option::is_none")]
    pub response: Option<MessageSchema<'a>>,
    /// Response schema name.
    #[serde(borrow, skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<Cow<'a, str>>,
}

impl<'a> Service<'a> {
    /// Creates a new service advertisement.
    pub fn new(id: u32, name: impl Into<Cow<'a, str>>, r#type: impl Into<Cow<'a, str>>) -> Self {
        Self {
            id,
            name: name.into(),
            r#type: r#type.into(),
            request: None,
            request_schema: Some("".into()),
            response: None,
            response_schema: Some("".into()),
        }
    }

    /// Adds a request schema to the service advertisement.
    ///
    /// This is the preferred method for setting request schemas. Note that calling
    /// [`Service::with_request_schema`] after this method will override the schema set here.
    pub fn with_request(
        mut self,
        encoding: impl Into<Cow<'a, str>>,
        schema: Schema<'a>,
    ) -> Result<Self, schema::EncodeError> {
        let schema_data = schema::encode_schema_data(&schema.encoding, schema.data)?;
        self.request = Some(MessageSchema::new(
            encoding,
            schema.name,
            schema.encoding,
            schema_data,
        ));
        self.request_schema = None;
        Ok(self)
    }

    /// Adds a response schema to the service advertisement.
    ///
    /// This is the preferred method for setting response schemas. Note that calling
    /// [`Service::with_response_schema`] after this method will override the schema set here.
    pub fn with_response(
        mut self,
        encoding: impl Into<Cow<'a, str>>,
        schema: Schema<'a>,
    ) -> Result<Self, schema::EncodeError> {
        let schema_data = schema::encode_schema_data(&schema.encoding, schema.data)?;
        self.response = Some(MessageSchema::new(
            encoding,
            schema.name,
            schema.encoding,
            schema_data,
        ));
        self.response_schema = None;
        Ok(self)
    }

    /// Adds a request schema name to the service advertisement.
    ///
    /// This method is provided for backwards compatibility. Prefer using [`Service::with_request`]
    /// instead. Note that this will override any schema set by [`Service::with_request`].
    #[must_use]
    pub fn with_request_schema(mut self, schema_name: impl Into<Cow<'a, str>>) -> Self {
        self.request = None;
        self.request_schema = Some(schema_name.into());
        self
    }

    /// Adds a response schema name to the service advertisement.
    ///
    /// This method is provided for backwards compatibility. Prefer using
    /// [`Service::with_response`] instead. Note that this will override any schema set by
    /// [`Service::with_response`].
    #[must_use]
    pub fn with_response_schema(mut self, schema_name: impl Into<Cow<'a, str>>) -> Self {
        self.response = None;
        self.response_schema = Some(schema_name.into());
        self
    }

    /// Returns an owned version of this service.
    pub fn into_owned(self) -> Service<'static> {
        Service {
            id: self.id,
            name: self.name.into_owned().into(),
            r#type: self.r#type.into_owned().into(),
            request: self.request.map(|r| r.into_owned()),
            request_schema: self.request_schema.map(|r| r.into_owned().into()),
            response: self.response.map(|r| r.into_owned()),
            response_schema: self.response_schema.map(|r| r.into_owned().into()),
        }
    }
}

/// A message schema for a [`Service`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageSchema<'a> {
    /// Message encoding.
    #[serde(borrow)]
    pub encoding: Cow<'a, str>,
    /// Schema name.
    #[serde(borrow)]
    pub schema_name: Cow<'a, str>,
    /// Schema encoding.
    #[serde(borrow)]
    pub schema_encoding: Cow<'a, str>,
    /// Schema data.
    ///
    /// This is the protocol-encoded form. You can use the [`MessageSchema::schema`] method to
    /// decode it.
    #[serde(borrow)]
    pub schema: Cow<'a, str>,
}

impl<'a> MessageSchema<'a> {
    /// Creates a new message schema.
    pub fn new(
        encoding: impl Into<Cow<'a, str>>,
        schema_name: impl Into<Cow<'a, str>>,
        schema_encoding: impl Into<Cow<'a, str>>,
        schema: impl Into<Cow<'a, str>>,
    ) -> Self {
        Self {
            encoding: encoding.into(),
            schema_name: schema_name.into(),
            schema_encoding: schema_encoding.into(),
            schema: schema.into(),
        }
    }

    /// Returns the decoded schema data.
    pub fn schema(&self) -> Result<Vec<u8>, schema::DecodeError> {
        schema::decode_schema_data(&self.schema_encoding, &self.schema)
    }

    /// Returns an owned version of this schema.
    pub fn into_owned(self) -> MessageSchema<'static> {
        MessageSchema {
            encoding: self.encoding.into_owned().into(),
            schema_name: self.schema_name.into_owned().into(),
            schema_encoding: self.schema_encoding.into_owned().into(),
            schema: Cow::Owned(self.schema.into_owned()),
        }
    }
}

impl<'a> TryFrom<MessageSchema<'a>> for Schema<'a> {
    type Error = schema::DecodeError;

    fn try_from(value: MessageSchema<'a>) -> Result<Self, Self::Error> {
        let schema = value.schema()?;
        Ok(Schema::new(
            value.schema_name,
            value.schema_encoding,
            schema,
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::server::ServerMessage;

    use super::*;

    fn message() -> AdvertiseServices<'static> {
        AdvertiseServices::new([
            // Service with both request and response schemas using the new format
            Service::new(10, "/s1", "my_type")
                .with_request(
                    "json",
                    Schema::new("request-schema", "jsonschema", br#"{"type": "object"}"#),
                )
                .unwrap()
                .with_response(
                    "json",
                    Schema::new("response-schema", "jsonschema", br#"{"type": "object"}"#),
                )
                .unwrap(),
            // Service with only request schema using the new format
            Service::new(20, "/s2", "other_type")
                .with_request(
                    "protobuf",
                    Schema::new("request-schema", "protobuf", &[0xde, 0xad, 0xbe, 0xef]),
                )
                .unwrap(),
            // Service with both request and response schemas using the old format
            Service::new(30, "/s3", "old_type")
                .with_request_schema("request-schema")
                .with_response_schema("response-schema"),
            // Service with request schema in new format and response in old format
            Service::new(40, "/s4", "mixed_type")
                .with_request(
                    "json",
                    Schema::new("request-schema", "jsonschema", br#"{"type": "object"}"#),
                )
                .unwrap()
                .with_response_schema("response-schema"),
            // Service with request schema in old format and response in new format
            Service::new(50, "/s5", "mixed_type")
                .with_request_schema("request-schema")
                .with_response(
                    "json",
                    Schema::new("response-schema", "jsonschema", br#"{"type": "object"}"#),
                )
                .unwrap(),
            // Service with schema overrides
            Service::new(60, "/s6", "override_type")
                .with_request(
                    "json",
                    Schema::new("request-schema", "jsonschema", br#"{"type": "object"}"#),
                )
                .unwrap()
                .with_request_schema("new-request-schema")
                .with_response_schema("response-schema")
                .with_response(
                    "json",
                    Schema::new(
                        "new-response-schema",
                        "jsonschema",
                        br#"{"type": "object"}"#,
                    ),
                )
                .unwrap(),
            // Service with default schemas
            Service::new(70, "/s7", "default_schemas"),
        ])
    }

    #[test]
    fn test_encode() {
        insta::assert_json_snapshot!(message());
    }

    #[test]
    fn test_roundtrip() {
        let orig = message();
        let buf = orig.to_string();
        let msg = ServerMessage::parse_json(&buf).unwrap();
        assert_eq!(msg, ServerMessage::AdvertiseServices(orig));
    }
}
