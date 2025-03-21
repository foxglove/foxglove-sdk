//! Definitions of client-to-server messages in ws-protocol.
//! Serializations are derived for testing.

use crate::{
    channel::ChannelId,
    websocket::service::{CallId, ServiceId},
};
use base64::{prelude::BASE64_STANDARD, Engine};
use bytes::{Buf, Bytes};
use serde::{Deserialize, Serialize};

use super::{is_known_binary_schema_encoding, server::Parameter};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ParseError {
    #[error("Unknown binary opcode {0}")]
    InvalidOpcode(u8),
    #[error("Buffer too short")]
    BufferTooShort,
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Base64(#[from] base64::DecodeError),
}

#[derive(Debug, PartialEq)]
pub(crate) enum ClientMessage {
    Subscribe(Subscribe),
    Unsubscribe(Unsubscribe),
    Advertise(ClientAdvertise),
    Unadvertise(ClientUnadvertise),
    MessageData(ClientMessageData),
    GetParameters(GetParameters),
    SetParameters(SetParameters),
    SubscribeParameterUpdates(ParameterNames),
    UnsubscribeParameterUpdates(ParameterNames),
    ServiceCallRequest(ServiceCallRequest),
    SubscribeConnectionGraph,
    UnsubscribeConnectionGraph,
    FetchAsset(FetchAsset),
}
impl ClientMessage {
    pub fn parse_json(json: &str) -> Result<Self, ParseError> {
        let msg = serde_json::from_str::<JsonMessage>(json)?;
        Self::try_from(msg)
    }

    pub fn parse_binary(mut data: Bytes) -> Result<Option<Self>, ParseError> {
        if data.is_empty() {
            Ok(None)
        } else {
            let opcode = data.get_u8();
            match BinaryOpcode::from_repr(opcode) {
                Some(BinaryOpcode::MessageData) => ClientMessageData::parse(data)
                    .map(ClientMessage::MessageData)
                    .map(Some),
                Some(BinaryOpcode::ServiceCallRequest) => ServiceCallRequest::parse(data)
                    .map(ClientMessage::ServiceCallRequest)
                    .map(Some),
                None => Err(ParseError::InvalidOpcode(opcode)),
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "op")]
#[serde(rename_all = "camelCase")]
enum JsonMessage {
    Subscribe(Subscribe),
    Unsubscribe(Unsubscribe),
    Advertise(JsonClientAdvertise),
    Unadvertise(ClientUnadvertise),
    GetParameters(GetParameters),
    SetParameters(SetParameters),
    SubscribeParameterUpdates(ParameterNames),
    UnsubscribeParameterUpdates(ParameterNames),
    SubscribeConnectionGraph,
    UnsubscribeConnectionGraph,
    FetchAsset(FetchAsset),
}

impl TryFrom<JsonMessage> for ClientMessage {
    type Error = ParseError;
    fn try_from(m: JsonMessage) -> Result<Self, ParseError> {
        match m {
            JsonMessage::Subscribe(m) => Ok(ClientMessage::Subscribe(m)),
            JsonMessage::Unsubscribe(m) => Ok(ClientMessage::Unsubscribe(m)),
            JsonMessage::Advertise(m) => Ok(ClientMessage::Advertise(m.try_into()?)),
            JsonMessage::Unadvertise(m) => Ok(ClientMessage::Unadvertise(m)),
            JsonMessage::GetParameters(m) => Ok(ClientMessage::GetParameters(m)),
            JsonMessage::SetParameters(m) => Ok(ClientMessage::SetParameters(m)),
            JsonMessage::SubscribeParameterUpdates(m) => {
                Ok(ClientMessage::SubscribeParameterUpdates(m))
            }
            JsonMessage::UnsubscribeParameterUpdates(m) => {
                Ok(ClientMessage::UnsubscribeParameterUpdates(m))
            }
            JsonMessage::SubscribeConnectionGraph => Ok(ClientMessage::SubscribeConnectionGraph),
            JsonMessage::UnsubscribeConnectionGraph => {
                Ok(ClientMessage::UnsubscribeConnectionGraph)
            }
            JsonMessage::FetchAsset(m) => Ok(ClientMessage::FetchAsset(m)),
        }
    }
}

/// A client channel ID.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Deserialize, Serialize)]
pub struct SubscriptionId(u32);

impl SubscriptionId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

impl From<SubscriptionId> for u32 {
    fn from(id: SubscriptionId) -> u32 {
        id.0
    }
}

impl std::fmt::Display for SubscriptionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[repr(u8)]
#[derive(strum::FromRepr)]
enum BinaryOpcode {
    MessageData = 1,
    ServiceCallRequest = 2,
}

// https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#subscribe
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Subscribe {
    pub subscriptions: Vec<Subscription>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Subscription {
    pub id: SubscriptionId,
    pub channel_id: ChannelId,
}

// https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#unsubscribe
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Unsubscribe {
    pub subscription_ids: Vec<SubscriptionId>,
}

// https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#client-advertise
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonClientAdvertise {
    pub channels: Vec<JsonClientChannel>,
}

#[derive(Debug, PartialEq)]
pub(crate) struct ClientAdvertise {
    pub channels: Vec<ClientChannel>,
}

impl TryFrom<JsonClientAdvertise> for ClientAdvertise {
    type Error = ParseError;
    fn try_from(msg: JsonClientAdvertise) -> Result<Self, ParseError> {
        Ok(ClientAdvertise {
            channels: msg
                .channels
                .into_iter()
                .map(ClientChannel::try_from)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonClientChannel {
    pub id: ClientChannelId,
    /// The topic name for this channel
    pub topic: String,
    /// The encoding of messages on this channel
    pub encoding: String,
    /// The name of the schema for this channel
    pub schema_name: String,
    /// The encoding of the schema data
    pub schema_encoding: Option<String>,
    /// May be base64-encoded depending on the schema_encoding.
    pub schema: Option<String>,
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

impl TryFrom<JsonClientChannel> for ClientChannel {
    type Error = ParseError;
    fn try_from(channel: JsonClientChannel) -> Result<Self, ParseError> {
        let schema = channel
            .schema
            .map(|schema| {
                if channel
                    .schema_encoding
                    .as_ref()
                    .is_some_and(is_known_binary_schema_encoding)
                {
                    BASE64_STANDARD.decode(schema).map_err(ParseError::Base64)
                } else {
                    Ok(schema.into_bytes())
                }
            })
            .transpose()?;
        Ok(ClientChannel {
            id: channel.id,
            topic: channel.topic,
            encoding: channel.encoding,
            schema_name: channel.schema_name,
            schema_encoding: channel.schema_encoding,
            schema,
        })
    }
}

// https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#client-unadvertise
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClientUnadvertise {
    pub channel_ids: Vec<ClientChannelId>,
}

// https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#client-message-data
#[derive(Debug, PartialEq)]
pub(crate) struct ClientMessageData {
    pub channel_id: ClientChannelId,
    pub payload: Bytes,
}
impl ClientMessageData {
    /// Parses a service call request from a binary buffer.
    ///
    /// The caller is responsible for stripping and validating the 1-byte opcode.
    fn parse(mut data: Bytes) -> Result<Self, ParseError> {
        // 4-byte channel id
        // n-byte payload
        if data.remaining() < 4 {
            return Err(ParseError::BufferTooShort);
        }
        let channel_id = data.get_u32_le();
        Ok(Self {
            channel_id: ClientChannelId::new(channel_id),
            payload: data,
        })
    }
}

// https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#get-parameters
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GetParameters {
    pub parameter_names: Vec<String>,
    pub id: Option<String>,
}

// https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#set-parameters
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SetParameters {
    pub parameters: Vec<Parameter>,
    pub id: Option<String>,
}

// https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#subscribe-parameter-update
// https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#unsubscribe-parameter-update
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ParameterNames {
    pub parameter_names: Vec<String>,
}

// https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#service-call-request
#[derive(Debug, PartialEq)]
pub(crate) struct ServiceCallRequest {
    pub service_id: ServiceId,
    pub call_id: CallId,
    pub encoding: String,
    pub payload: Bytes,
}
impl ServiceCallRequest {
    /// Parses a service call request from a binary buffer.
    ///
    /// The caller is responsible for stripping and validating the 1-byte opcode.
    fn parse(mut data: Bytes) -> Result<Self, ParseError> {
        // 4-byte service id
        // 4-byte call id
        // 4-byte encoding length
        if data.remaining() < 12 {
            return Err(ParseError::BufferTooShort);
        }
        let service_id = data.get_u32_le();
        let call_id = data.get_u32_le();
        let encoding_length = data.get_u32_le() as usize;
        if data.remaining() < encoding_length {
            return Err(ParseError::BufferTooShort);
        }
        let encoding = std::str::from_utf8(&data[..encoding_length])?.to_string();
        data.advance(encoding_length);
        Ok(Self {
            service_id: ServiceId::new(service_id),
            call_id: CallId::new(call_id),
            encoding,
            payload: data,
        })
    }
}

// https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#fetch-asset
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FetchAsset {
    pub uri: String,
    pub request_id: u32,
}

#[cfg(test)]
mod tests {
    use super::super::server::{ParameterType, ParameterValue};
    use super::*;

    use assert_matches::assert_matches;
    use bytes::{BufMut, BytesMut};
    use serde_json::json;

    #[test]
    fn test_parse_subscribe() {
        let msg = json!({
          "op": "subscribe",
          "subscriptions": [
            { "id": 0, "channelId": 3 },
            { "id": 1, "channelId": 5 }
          ]
        })
        .to_string();

        let parsed = ClientMessage::parse_json(&msg).unwrap();
        assert_eq!(
            parsed,
            ClientMessage::Subscribe(Subscribe {
                subscriptions: vec![
                    Subscription {
                        id: SubscriptionId::new(0),
                        channel_id: ChannelId::new(3),
                    },
                    Subscription {
                        id: SubscriptionId::new(1),
                        channel_id: ChannelId::new(5),
                    },
                ]
            })
        );
    }

    #[test]
    fn test_parse_unsubscribe() {
        let msg = json!({
          "op": "unsubscribe",
          "subscriptionIds": [0, 1]
        })
        .to_string();

        let parsed = ClientMessage::parse_json(&msg).unwrap();
        assert_eq!(
            parsed,
            ClientMessage::Unsubscribe(Unsubscribe {
                subscription_ids: vec![SubscriptionId::new(0), SubscriptionId::new(1)],
            })
        );
    }

    #[test]
    fn test_parse_advertise() {
        let msg = json!({
            "op": "advertise",
            "channels": [
                {
                    "id": 1,
                    "topic": "/test",
                    "encoding": "json",
                    "schemaName": "test",
                },
                {
                    "id": 2,
                    "topic": "/test",
                    "encoding": "json",
                    "schemaName": "test",
                    "schemaEncoding": "unknown",
                    "schema": "abc",
                },
                {
                    "id": 3,
                    "topic": "/test",
                    "encoding": "json",
                    "schemaName": "test",
                    "schemaEncoding": "protobuf",
                    "schema": BASE64_STANDARD.encode("abc"),
                }
            ]
        })
        .to_string();

        let parsed = ClientMessage::parse_json(&msg).unwrap();
        assert_eq!(
            parsed,
            ClientMessage::Advertise(ClientAdvertise {
                channels: vec![
                    ClientChannel {
                        id: ClientChannelId::new(1),
                        topic: "/test".to_string(),
                        encoding: "json".to_string(),
                        schema_name: "test".to_string(),
                        schema_encoding: None,
                        schema: None,
                    },
                    ClientChannel {
                        id: ClientChannelId::new(2),
                        topic: "/test".to_string(),
                        encoding: "json".to_string(),
                        schema_name: "test".to_string(),
                        schema_encoding: Some("unknown".to_string()),
                        schema: Some(b"abc".to_vec()),
                    },
                    ClientChannel {
                        id: ClientChannelId::new(3),
                        topic: "/test".to_string(),
                        encoding: "json".to_string(),
                        schema_name: "test".to_string(),
                        schema_encoding: Some("protobuf".to_string()),
                        schema: Some(b"abc".to_vec()),
                    },
                ],
            }),
        );
    }

    #[test]
    fn test_parse_advertise_fail() {
        let msg = json!({
            "op": "advertise",
            "channels": [
                {
                    "id": 1,
                    "topic": "/test",
                    "encoding": "json",
                    "schemaName": "test",
                },
                {
                    "id": 2,
                    "topic": "/test",
                    "encoding": "json",
                    "schemaName": "test",
                    "schemaEncoding": "protobuf",
                    "schema": "~~~invalid base64~~~",
                },
            ]
        })
        .to_string();

        assert_matches!(
            ClientMessage::parse_json(&msg),
            Err(ParseError::Base64(base64::DecodeError::InvalidByte(
                0, b'~'
            )))
        );
    }

    #[test]
    fn test_parse_unadvertise() {
        let msg = json!({
            "op": "unadvertise",
            "channelIds": [1, 2]
        })
        .to_string();

        let parsed = ClientMessage::parse_json(&msg).unwrap();
        assert_eq!(
            parsed,
            ClientMessage::Unadvertise(ClientUnadvertise {
                channel_ids: vec![ClientChannelId::new(1), ClientChannelId::new(2)],
            })
        );
    }

    #[test]
    fn test_invalid_opcode() {
        let msg = Bytes::from_static(&[42u8]);
        assert_matches!(
            ClientMessage::parse_binary(msg),
            Err(ParseError::InvalidOpcode(42u8))
        );
    }

    #[test]
    fn test_parse_message_data() {
        let mut msg = BytesMut::new();
        msg.put_u8(BinaryOpcode::MessageData as u8);
        msg.put_u32_le(42);
        msg.put(b"payload".as_slice());

        let parsed = ClientMessage::parse_binary(msg.into()).unwrap();
        assert_eq!(
            parsed,
            Some(ClientMessage::MessageData(ClientMessageData {
                channel_id: ClientChannelId::new(42),
                payload: Bytes::from_static(b"payload"),
            }))
        )
    }

    #[test]
    fn test_get_parameters() {
        let msg = json!({
          "op": "getParameters",
          "parameterNames": [
            "/int_param",
            "/float_param",
            "/string_param",
            "/node/nested_ints_param"
          ],
          "id": "request-123"
        })
        .to_string();

        let parsed = ClientMessage::parse_json(&msg).unwrap();
        assert_eq!(
            parsed,
            ClientMessage::GetParameters(GetParameters {
                parameter_names: vec![
                    "/int_param".into(),
                    "/float_param".into(),
                    "/string_param".into(),
                    "/node/nested_ints_param".into(),
                ],
                id: Some("request-123".into())
            })
        );
    }

    #[test]
    fn test_set_parameters() {
        let msg = json!({
          "op": "setParameters",
          "parameters": [
            { "name": "/int_param", "value": 3 },
            { "name": "/float_param", "value": 4.1 },
            { "name": "/byte_array_param", "value": "QUJDRA==", "type": "byte_array" },
            { "name": "/float_param_int", "value": 3, "type": "float64" },
            { "name": "/float_array_param", "value": [1.1, 2, 3.3], "type": "float64_array" },
          ],
          "id": "request-456"
        })
        .to_string();

        let parsed = ClientMessage::parse_json(&msg).unwrap();
        assert_eq!(
            parsed,
            ClientMessage::SetParameters(SetParameters {
                parameters: vec![
                    Parameter {
                        name: "/int_param".into(),
                        value: Some(ParameterValue::Number(3.0)),
                        r#type: None
                    },
                    Parameter {
                        name: "/float_param".into(),
                        value: Some(ParameterValue::Number(4.1)),
                        r#type: None
                    },
                    Parameter {
                        name: "/byte_array_param".into(),
                        value: Some(ParameterValue::String(b"ABCD".to_vec())),
                        r#type: Some(ParameterType::ByteArray),
                    },
                    Parameter {
                        name: "/float_param_int".into(),
                        value: Some(ParameterValue::Number(3.0)),
                        r#type: Some(ParameterType::Float64),
                    },
                    Parameter {
                        name: "/float_array_param".into(),
                        value: Some(ParameterValue::Array(
                            [1.1, 2.0, 3.3]
                                .into_iter()
                                .map(ParameterValue::Number)
                                .collect()
                        )),
                        r#type: Some(ParameterType::Float64Array),
                    },
                ],
                id: Some("request-456".into()),
            })
        );
    }

    #[test]
    fn test_subscribe_parameter_updates() {
        let msg = json!({
          "op": "subscribeParameterUpdates",
          "parameterNames": [
            "/int_param",
            "/float_param",
            "/string_param",
            "/node/nested_ints_param"
          ]
        })
        .to_string();

        let parsed = ClientMessage::parse_json(&msg).unwrap();
        assert_eq!(
            parsed,
            ClientMessage::SubscribeParameterUpdates(ParameterNames {
                parameter_names: vec![
                    "/int_param".into(),
                    "/float_param".into(),
                    "/string_param".into(),
                    "/node/nested_ints_param".into(),
                ]
            })
        )
    }

    #[test]
    fn test_unsubscribe_parameter_updates() {
        let msg = json!({
          "op": "unsubscribeParameterUpdates",
          "parameterNames": [
            "/int_param",
            "/float_param",
            "/string_param",
            "/node/nested_ints_param"
          ]
        })
        .to_string();

        let parsed = ClientMessage::parse_json(&msg).unwrap();
        assert_eq!(
            parsed,
            ClientMessage::UnsubscribeParameterUpdates(ParameterNames {
                parameter_names: vec![
                    "/int_param".into(),
                    "/float_param".into(),
                    "/string_param".into(),
                    "/node/nested_ints_param".into(),
                ]
            })
        )
    }

    #[test]
    fn test_parse_subscribe_connection_graph() {
        let msg = json!({
          "op": "subscribeConnectionGraph"
        })
        .to_string();

        let parsed = ClientMessage::parse_json(&msg).unwrap();
        assert_eq!(parsed, ClientMessage::SubscribeConnectionGraph);
    }

    #[test]
    fn test_parse_unsubscribe_connection_graph() {
        let msg = json!({
          "op": "unsubscribeConnectionGraph"
        })
        .to_string();

        let parsed = ClientMessage::parse_json(&msg).unwrap();
        assert_eq!(parsed, ClientMessage::UnsubscribeConnectionGraph);
    }

    #[test]
    fn test_parse_service_call_request() {
        let mut msg = BytesMut::new();
        msg.put_u8(BinaryOpcode::ServiceCallRequest as u8);
        msg.put_u32_le(42); // service id
        msg.put_u32_le(314); // call id
        msg.put_u32_le(3); // encoding length
        msg.put(b"raw".as_slice());
        msg.put(b"payload".as_slice());

        let parsed = ClientMessage::parse_binary(msg.into()).unwrap();
        assert_eq!(
            parsed,
            Some(ClientMessage::ServiceCallRequest(ServiceCallRequest {
                service_id: ServiceId::new(42),
                call_id: CallId::new(314),
                encoding: "raw".into(),
                payload: Bytes::from_static(b"payload"),
            }))
        );
    }

    #[test]
    fn test_parse_fetch_asset() {
        let msg = json!({
          "op": "fetchAsset",
          "uri": "package://foo/robot.urdf",
          "requestId": 123
        })
        .to_string();

        let parsed = ClientMessage::parse_json(&msg).unwrap();
        assert_eq!(
            parsed,
            ClientMessage::FetchAsset(FetchAsset {
                uri: "package://foo/robot.urdf".into(),
                request_id: 123
            })
        );
    }
}
