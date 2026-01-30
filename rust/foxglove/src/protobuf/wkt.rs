//! ProtobufField implementations for well-known types (Timestamp, Duration).

use prost_types::field_descriptor_proto::Type as ProstFieldType;

use super::ProtobufField;
use crate::schemas::{Duration, Timestamp};

/// Creates a google.protobuf.Timestamp FileDescriptorProto.
fn timestamp_file_descriptor() -> prost_types::FileDescriptorProto {
    let mut message = prost_types::DescriptorProto::default();
    message.name = Some("Timestamp".to_string());

    // seconds field
    let mut seconds_field = prost_types::FieldDescriptorProto::default();
    seconds_field.name = Some("seconds".to_string());
    seconds_field.number = Some(1);
    seconds_field.r#type = Some(ProstFieldType::Int64 as i32);
    seconds_field.label =
        Some(prost_types::field_descriptor_proto::Label::Optional as i32);
    message.field.push(seconds_field);

    // nanos field
    let mut nanos_field = prost_types::FieldDescriptorProto::default();
    nanos_field.name = Some("nanos".to_string());
    nanos_field.number = Some(2);
    nanos_field.r#type = Some(ProstFieldType::Int32 as i32);
    nanos_field.label =
        Some(prost_types::field_descriptor_proto::Label::Optional as i32);
    message.field.push(nanos_field);

    prost_types::FileDescriptorProto {
        name: Some("google/protobuf/timestamp.proto".to_string()),
        package: Some("google.protobuf".to_string()),
        message_type: vec![message],
        syntax: Some("proto3".to_string()),
        ..Default::default()
    }
}

/// Creates a google.protobuf.Duration FileDescriptorProto.
fn duration_file_descriptor() -> prost_types::FileDescriptorProto {
    let mut message = prost_types::DescriptorProto::default();
    message.name = Some("Duration".to_string());

    // seconds field
    let mut seconds_field = prost_types::FieldDescriptorProto::default();
    seconds_field.name = Some("seconds".to_string());
    seconds_field.number = Some(1);
    seconds_field.r#type = Some(ProstFieldType::Int64 as i32);
    seconds_field.label =
        Some(prost_types::field_descriptor_proto::Label::Optional as i32);
    message.field.push(seconds_field);

    // nanos field
    let mut nanos_field = prost_types::FieldDescriptorProto::default();
    nanos_field.name = Some("nanos".to_string());
    nanos_field.number = Some(2);
    nanos_field.r#type = Some(ProstFieldType::Int32 as i32);
    nanos_field.label =
        Some(prost_types::field_descriptor_proto::Label::Optional as i32);
    message.field.push(nanos_field);

    prost_types::FileDescriptorProto {
        name: Some("google/protobuf/duration.proto".to_string()),
        package: Some("google.protobuf".to_string()),
        message_type: vec![message],
        syntax: Some("proto3".to_string()),
        ..Default::default()
    }
}

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
        Some(timestamp_file_descriptor())
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
        Some(duration_file_descriptor())
    }

    fn encoded_len(&self) -> usize {
        use prost::Message;
        let inner_len = Message::encoded_len(self);
        prost::encoding::encoded_len_varint(inner_len as u64) + inner_len
    }
}
