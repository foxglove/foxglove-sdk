use anyhow::{anyhow, Result};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use serde::Deserialize;
use tokio_tungstenite::tungstenite::{http::HeaderValue, Message};

pub const SDK_SUBPROTOCOL: HeaderValue = HeaderValue::from_static("foxglove.sdk.v1");

pub enum ServerMessage {
    ServerInfo,
    AdvertiseServices(AdvertiseServices),
    UnadvertiseServices(UnadvertiseServices),
    ServiceCallResponse(ServiceCallResponse),
    ServiceCallFailure(ServiceCallFailure),
}
impl ServerMessage {
    pub fn parse_message(message: Message) -> Result<Option<Self>> {
        match message {
            Message::Text(bytes) => Self::parse_json(bytes.as_str()).map(Some),
            Message::Binary(bytes) => Self::parse_binary(bytes),
            _ => Err(anyhow!("unhandled message {message:?}")),
        }
    }

    fn parse_json(json: &str) -> Result<Self> {
        let msg = serde_json::from_str::<JsonMessage>(json)?;
        Ok(Self::from(msg))
    }

    fn parse_binary(mut data: Bytes) -> Result<Option<Self>> {
        if data.is_empty() {
            Ok(None)
        } else {
            let opcode = data.get_u8();
            match BinaryOpcode::from_repr(opcode) {
                Some(BinaryOpcode::ServiceCallResponse) => ServiceCallResponse::parse(data)
                    .map(ServerMessage::ServiceCallResponse)
                    .map(Some),
                _ => Err(anyhow!("invalid opcode {opcode}")),
            }
        }
    }
}

#[repr(u8)]
#[derive(strum::FromRepr)]
enum BinaryOpcode {
    ServiceCallRequest = 2,
    ServiceCallResponse = 3,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", tag = "op")]
enum JsonMessage {
    ServerInfo,
    AdvertiseServices(AdvertiseServices),
    UnadvertiseServices(UnadvertiseServices),
    ServiceCallFailure(ServiceCallFailure),
}

impl From<JsonMessage> for ServerMessage {
    fn from(m: JsonMessage) -> Self {
        match m {
            JsonMessage::ServerInfo => ServerMessage::ServerInfo,
            JsonMessage::AdvertiseServices(m) => ServerMessage::AdvertiseServices(m),
            JsonMessage::UnadvertiseServices(m) => ServerMessage::UnadvertiseServices(m),
            JsonMessage::ServiceCallFailure(m) => ServerMessage::ServiceCallFailure(m),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvertiseServices {
    pub services: Vec<AdvertiseService>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvertiseService {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnadvertiseServices {
    pub service_ids: Vec<u32>,
}

#[derive(Debug)]
pub struct ServiceCallRequest<'a> {
    pub service_id: u32,
    pub call_id: u32,
    pub encoding: &'a str,
    pub payload: Bytes,
}
impl ServiceCallRequest<'_> {
    pub fn encode(self) -> Message {
        let encoding_raw = self.encoding.as_bytes();
        let mut buf = BytesMut::new();
        buf.put_u8(BinaryOpcode::ServiceCallRequest as u8);
        buf.put_u32_le(self.service_id);
        buf.put_u32_le(self.call_id);
        buf.put_u32_le(encoding_raw.len() as u32);
        buf.put(encoding_raw);
        buf.put(self.payload);
        Message::Binary(buf.into())
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct ServiceCallResponse {
    pub service_id: u32,
    pub call_id: u32,
    pub encoding: String,
    pub payload: Bytes,
}
impl ServiceCallResponse {
    fn parse(mut data: Bytes) -> Result<Self> {
        // 4-byte service id, call id, and encoding length.
        if data.remaining() < 12 {
            return Err(anyhow!("Buffer too short"));
        }
        let service_id = data.get_u32_le();
        let call_id = data.get_u32_le();
        let encoding_length = data.get_u32_le() as usize;
        if data.remaining() < encoding_length {
            return Err(anyhow!("Buffer too short"));
        }
        let encoding = std::str::from_utf8(&data[..encoding_length])?.to_string();
        data.advance(encoding_length);
        Ok(Self {
            service_id,
            call_id,
            encoding,
            payload: data,
        })
    }
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceCallFailure {
    pub service_id: u32,
    pub call_id: u32,
    pub message: String,
}
