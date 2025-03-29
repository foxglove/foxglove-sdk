use bytes::BytesMut;
use foxglove::{Encode, Schema};
use foxglove_derive::Loggable;
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor};

#[derive(Loggable)]
struct TestMessage {
    number: u64,
    float32: f32,
}

#[derive(Loggable)]
struct TestMessageVector {
    numbers: Vec<u64>,
}

#[test]
fn test_single_u64_field_serialization() {
    let test_struct = TestMessage {
        number: 42,
        float32: 1234.5678,
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("Failed to encode");

    let schema = TestMessage::get_schema().expect("Failed to get schema");
    assert_eq!(schema.encoding, "protobuf");

    assert_eq!(schema.name, "testmessage.TestMessage");

    let message_descriptor = get_message_descriptor(&schema);

    let deserialized_message = DynamicMessage::decode(message_descriptor.clone(), buf.as_ref())
        .expect("Failed to deserialize");

    {
        let field_descriptor = message_descriptor
            .get_field_by_name("number")
            .expect("Field 'number' not found");
        assert_eq!(field_descriptor.name(), "number");

        let field_value = deserialized_message.get_field(&field_descriptor);
        let number_value = field_value.as_u64().expect("Field value is not a u64");
        assert_eq!(number_value, 42);
    }

    {
        let field_descriptor = message_descriptor
            .get_field_by_name("float32")
            .expect("Field 'float32' not found");

        let field_value = deserialized_message.get_field(&field_descriptor);
        let number_value = field_value.as_f32().expect("Field value is not a f32");
        assert_eq!(number_value, 1234.5678);
    }
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

fn get_message_descriptor(schema: &Schema) -> MessageDescriptor {
    let descriptor_set = prost_types::FileDescriptorSet::decode(schema.data.as_ref())
        .expect("Failed to decode descriptor set");

    let pool = DescriptorPool::from_file_descriptor_set(descriptor_set).unwrap();

    pool.get_message_by_name(schema.name.as_str())
        .unwrap_or_else(|| panic!("Failed to get message descriptor for {}", schema.name))
}
