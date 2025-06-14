// This file is @generated
use foxglove::{Encode, Schema};
use bytes::BufMut;

impl Encode for super::Apple {
    type Error = ::prost::EncodeError;

    fn get_schema() -> Option<Schema> {
        Some(Schema::new(
            "fruit.Apple",
            "protobuf",
            super::descriptors::APPLE,
        ))
    }

    fn get_message_encoding() -> String {
        "protobuf".to_string()
    }

    fn encode(&self, buf: &mut impl BufMut) -> Result<(), prost::EncodeError> {
        ::prost::Message::encode(self, buf)
    }

    fn encoded_len(&self) -> Option<usize> { Some(::prost::Message::encoded_len(self)) }
}
