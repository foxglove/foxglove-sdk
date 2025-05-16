use ::foxglove::{Encode, Schema};
use bytes::BytesMut;
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor};

mod common;
use common::FixedSizeBuffer;

// Ensure the macro properly references the foxglove crate
mod foxglove {}

#[derive(Encode)]
struct TestMessagePrimitives {
    u64: u64,
    u32: u32,
    u16: u16,
    u8: u8,
    i64: i64,
    i32: i32,
    i16: i16,
    i8: i8,
    f64: f64,
    f32: f32,
    bool: bool,
}

#[derive(Encode)]
struct RepeatedPrimitive {
    a: u32,
    b: u32,
}

#[derive(Encode)]
struct TestMessageBytes {
    bytes: bytes::Bytes,
}

#[derive(Encode)]
struct TestMessageVector {
    numbers: Vec<u64>,
}

#[derive(Encode)]
struct GenericMessage<T> {
    val: T,
}

#[test]
fn test_primitive_serialization() {
    let test_struct = TestMessagePrimitives {
        u64: u32::MAX as u64 + 1,
        u32: 42,
        u16: 43,
        u8: 44,
        i64: i64::MIN,
        i32: 42,
        i16: 43,
        i8: -127,
        f64: -33.5,
        f32: 1234.5678,
        bool: true,
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("Failed to encode");

    let schema = TestMessagePrimitives::get_schema().expect("Failed to get schema");
    assert_eq!(schema.encoding, "protobuf");
    assert_eq!(schema.name, "testmessageprimitives.TestMessagePrimitives");

    let message_descriptor = get_message_descriptor(&schema);
    let deserialized_message = DynamicMessage::decode(message_descriptor.clone(), buf.as_ref())
        .expect("Failed to deserialize");

    let field_descriptor = message_descriptor
        .get_field_by_name("u64")
        .expect("Field 'u64' not found");
    let field_value = deserialized_message.get_field(&field_descriptor);
    let number_value = field_value.as_u64().expect("Field value is not a u64");
    assert_eq!(field_descriptor.name(), "u64");
    assert_eq!(number_value, (u32::MAX as u64 + 1));

    let unsigned_32_types = [("u8", 44), ("u16", 43), ("u32", 42)];
    for (field_name, expected_value) in unsigned_32_types {
        let field_descriptor = message_descriptor
            .get_field_by_name(field_name)
            .unwrap_or_else(|| panic!("Field '{}' not found", field_name));
        let field_value = deserialized_message.get_field(&field_descriptor);
        let number_value = field_value.as_u32().expect("Field value is not a u32");
        assert_eq!(field_descriptor.name(), field_name);
        assert_eq!(number_value, expected_value);
    }

    let field_descriptor = message_descriptor
        .get_field_by_name("i64")
        .expect("Field 'i64' not found");
    let field_value = deserialized_message.get_field(&field_descriptor);
    let number_value = field_value.as_i64().expect("Field value is not a i64");
    assert_eq!(field_descriptor.name(), "i64");
    assert_eq!(number_value, i64::MIN);

    let signed_32_types = [("i8", -127), ("i16", 43), ("i32", 42)];
    for (field_name, expected_value) in signed_32_types {
        let field_descriptor = message_descriptor
            .get_field_by_name(field_name)
            .unwrap_or_else(|| panic!("Field '{}' not found", field_name));
        let field_value = deserialized_message.get_field(&field_descriptor);
        let number_value = field_value.as_i32().expect("Field value is not a i32");
        assert_eq!(field_descriptor.name(), field_name);
        assert_eq!(number_value, expected_value);
    }

    let field_descriptor = message_descriptor
        .get_field_by_name("f32")
        .expect("Field 'f32' not found");
    let field_value = deserialized_message.get_field(&field_descriptor);
    let number_value = field_value.as_f32().expect("Field value is not a f32");
    assert_eq!(field_descriptor.name(), "f32");
    assert_eq!(number_value, 1234.5678);

    let field_descriptor = message_descriptor
        .get_field_by_name("f64")
        .expect("Field 'f64' not found");
    let field_value = deserialized_message.get_field(&field_descriptor);
    let number_value = field_value.as_f64().expect("Field value is not a f64");
    assert_eq!(field_descriptor.name(), "f64");
    assert_eq!(number_value, -33.5);

    let field_descriptor = message_descriptor
        .get_field_by_name("bool")
        .expect("Field 'bool' not found");
    let field_value = deserialized_message.get_field(&field_descriptor);
    let bool_value = field_value.as_bool().expect("Field value is not a bool");
    assert_eq!(field_descriptor.name(), "bool");
    assert!(bool_value);
}

#[test]
fn test_repeated_primitive_serialization() {
    let test_struct = RepeatedPrimitive { a: 1, b: 2 };
    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("Failed to encode");

    let schema = RepeatedPrimitive::get_schema().expect("Failed to get schema");
    assert_eq!(schema.encoding, "protobuf");
    assert_eq!(schema.name, "repeatedprimitive.RepeatedPrimitive");

    let message_descriptor = get_message_descriptor(&schema);
    let deserialized_message = DynamicMessage::decode(message_descriptor.clone(), buf.as_ref())
        .expect("Failed to deserialize");

    let fields = [("a", 1), ("b", 2)];
    for (field_name, expected_value) in fields {
        let field_descriptor = message_descriptor
            .get_field_by_name(field_name)
            .unwrap_or_else(|| panic!("Field '{}' not found", field_name));
        let field_value = deserialized_message.get_field(&field_descriptor);
        let number_value = field_value.as_u32().expect("Field value is not a u32");
        assert_eq!(field_descriptor.name(), field_name);
        assert_eq!(number_value, expected_value);
    }
}

#[test]
fn test_bytes_serialization() {
    let test_struct = TestMessageBytes {
        bytes: bytes::Bytes::from_static(&[1, 2, 3]),
    };
    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("Failed to encode");

    let schema = TestMessageBytes::get_schema().expect("Failed to get schema");
    assert_eq!(schema.encoding, "protobuf");
    assert_eq!(schema.name, "testmessagebytes.TestMessageBytes");

    let message_descriptor = get_message_descriptor(&schema);
    let deserialized_message = DynamicMessage::decode(message_descriptor.clone(), buf.as_ref())
        .expect("Failed to deserialize");

    let field_descriptor = message_descriptor
        .get_field_by_name("bytes")
        .expect("Field 'bytes' not found");
    let field_value = deserialized_message.get_field(&field_descriptor);
    let bytes_value = field_value.as_bytes().expect("Field value is not bytes");
    assert_eq!(field_descriptor.name(), "bytes");
    assert_eq!(bytes_value.as_ref(), &[1, 2, 3]);
}

#[test]
fn test_insufficient_bytes_buffer_errors() {
    let test_struct = TestMessageBytes {
        bytes: bytes::Bytes::from_static(&[1, 2, 3, 4]),
    };
    let mut buf = FixedSizeBuffer::with_capacity(3);
    let result = test_struct.encode(&mut buf);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "Encoding error: insufficient buffer"
    );
}

#[test]
fn test_vector_of_u64_field_serialization() {
    let test_struct = TestMessageVector {
        numbers: vec![42, 84, 126],
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("Failed to encode");

    let schema = TestMessageVector::get_schema().expect("Failed to get schema");
    assert_eq!(schema.encoding, "protobuf");
    assert_eq!(schema.name, "testmessagevector.TestMessageVector");

    let descriptor_set = prost_types::FileDescriptorSet::decode(schema.data.as_ref())
        .expect("Failed to decode descriptor set");
    let file = &descriptor_set.file[0];

    // Verify the message has a repeated field
    let message_type = &file.message_type[0];
    assert_eq!(message_type.name.as_ref().unwrap(), "TestMessageVector");

    let field = &message_type.field[0];
    assert_eq!(field.name.as_ref().unwrap(), "numbers");
    assert_eq!(
        field.label.unwrap(),
        prost_types::field_descriptor_proto::Label::Repeated as i32
    );
    assert_eq!(
        field.r#type.unwrap(),
        prost_types::field_descriptor_proto::Type::Uint64 as i32
    );

    // Deserialize and verify
    let message_descriptor = get_message_descriptor(&schema);
    let deserialized_message = DynamicMessage::decode(message_descriptor.clone(), buf.as_ref())
        .expect("Failed to deserialize vector message");

    let field_descriptor = message_descriptor
        .get_field_by_name("numbers")
        .expect("Field 'numbers' not found");
    assert_eq!(field_descriptor.name(), "numbers");
    assert!(
        field_descriptor.is_list(),
        "Field should be a repeated list"
    );

    // Get the list value and verify each element
    let field_value = deserialized_message.get_field(&field_descriptor);
    let list_value = field_value.as_list().expect("Field value is not a list");

    assert_eq!(list_value.len(), 3, "Vector should have 3 elements");
    assert_eq!(list_value[0].as_u64().unwrap(), 42);
    assert_eq!(list_value[1].as_u64().unwrap(), 84);
    assert_eq!(list_value[2].as_u64().unwrap(), 126);
}

#[test]
fn test_generics() {
    let test_struct = GenericMessage::<u32> { val: 42 };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("Failed to encode");

    let schema = GenericMessage::<u32>::get_schema().expect("Failed to get schema");
    assert_eq!(schema.encoding, "protobuf");
    assert_eq!(schema.name, "genericmessage.GenericMessage");

    let message_descriptor = get_message_descriptor(&schema);
    let deserialized_message = DynamicMessage::decode(message_descriptor.clone(), buf.as_ref())
        .expect("Failed to deserialize");

    let field_descriptor = message_descriptor
        .get_field_by_name("val")
        .expect("Field 'val' not found");

    let field_value = deserialized_message.get_field(&field_descriptor);
    let number_value = field_value.as_u32().expect("Field value is not a u32");
    assert_eq!(field_descriptor.name(), "val");
    assert_eq!(number_value, 42);
}

fn get_message_descriptor(schema: &Schema) -> MessageDescriptor {
    let descriptor_set = prost_types::FileDescriptorSet::decode(schema.data.as_ref())
        .expect("Failed to decode descriptor set");

    let pool = DescriptorPool::from_file_descriptor_set(descriptor_set).unwrap();

    pool.get_message_by_name(schema.name.as_str())
        .unwrap_or_else(|| panic!("Failed to get message descriptor for {}", schema.name))
}
