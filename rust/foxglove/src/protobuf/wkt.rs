//! ProtobufField implementations for well-known types (Timestamp, Duration).

use prost_types::field_descriptor_proto::Type as ProstFieldType;

use super::ProtobufField;
use crate::schemas::{Duration, Timestamp};

impl ProtobufField for Timestamp {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Message
    }

    fn wire_type() -> u32 {
        prost::encoding::WireType::LengthDelimited as u32
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        use prost::Message;
        // Write length prefix, then raw message content
        let len = Message::encoded_len(self);
        prost::encoding::encode_varint(len as u64, buf);
        self.encode_raw(buf);
    }

    fn type_name() -> Option<String> {
        Some(".google.protobuf.Timestamp".to_string())
    }

    fn encoded_len(&self) -> usize {
        use prost::Message;
        let inner_len = Message::encoded_len(self);
        prost::encoding::encoded_len_varint(inner_len as u64) + inner_len
    }
}

impl ProtobufField for Duration {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Message
    }

    fn wire_type() -> u32 {
        prost::encoding::WireType::LengthDelimited as u32
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        use prost::Message;
        // Write length prefix, then raw message content
        let len = Message::encoded_len(self);
        prost::encoding::encode_varint(len as u64, buf);
        self.encode_raw(buf);
    }

    fn type_name() -> Option<String> {
        Some(".google.protobuf.Duration".to_string())
    }

    fn encoded_len(&self) -> usize {
        use prost::Message;
        let inner_len = Message::encoded_len(self);
        prost::encoding::encoded_len_varint(inner_len as u64) + inner_len
    }
}
