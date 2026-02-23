//! ProtobufField implementations for well-known types (Timestamp, Duration).

use std::sync::LazyLock;

use prost::Message;
use prost_types::field_descriptor_proto::Type as ProstFieldType;

use super::ProtobufField;
use crate::messages::{Duration, Timestamp, descriptors};

/// Decodes a FileDescriptorSet from binary and returns the first FileDescriptorProto.
fn decode_file_descriptor(bytes: &[u8]) -> prost_types::FileDescriptorProto {
    let fds = prost_types::FileDescriptorSet::decode(bytes).expect("invalid file descriptor set");
    fds.file
        .into_iter()
        .next()
        .expect("empty file descriptor set")
}

static TIMESTAMP_FD: LazyLock<prost_types::FileDescriptorProto> =
    LazyLock::new(|| decode_file_descriptor(descriptors::TIMESTAMP));

static DURATION_FD: LazyLock<prost_types::FileDescriptorProto> =
    LazyLock::new(|| decode_file_descriptor(descriptors::DURATION));

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

    fn file_descriptor() -> Option<prost_types::FileDescriptorProto> {
        Some(TIMESTAMP_FD.clone())
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

    fn file_descriptor() -> Option<prost_types::FileDescriptorProto> {
        Some(DURATION_FD.clone())
    }

    fn encoded_len(&self) -> usize {
        use prost::Message;
        let inner_len = Message::encoded_len(self);
        prost::encoding::encoded_len_varint(inner_len as u64) + inner_len
    }
}
