use bytes::BytesMut;
use foxglove::{Encode, Schema};
use foxglove_derive::Loggable;
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor};

#[derive(Loggable)]
struct TestMessage {
    field: String,
}

#[test]
fn test_single_string_field_serialization() {
    let test_struct = TestMessage {
        field: "Hello, world!".to_string(),
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("Failed to encode");

    let schema = TestMessage::get_schema().expect("Failed to get schema");
    assert_eq!(schema.encoding, "protobuf");

    assert_eq!(schema.name, "testmessage.TestMessage");

    let message_descriptor = get_message_descriptor(&schema);

    let deserialized_message = DynamicMessage::decode(message_descriptor.clone(), buf.as_ref())
        .expect("Failed to deserialize");

    let field_descriptor = message_descriptor
        .get_field_by_name("field")
        .expect("Field 'field' not found");
    assert_eq!(field_descriptor.name(), "field");

    let field_value = deserialized_message.get_field(&field_descriptor);
    let string_value = field_value.as_str().expect("Field value is not a string");
    assert_eq!(string_value, "Hello, world!");
}

fn get_message_descriptor(schema: &Schema) -> MessageDescriptor {
    let descriptor_set = prost_types::FileDescriptorSet::decode(schema.data.as_ref())
        .expect("Failed to decode descriptor set");

    let pool = DescriptorPool::from_file_descriptor_set(descriptor_set).unwrap();

    pool.get_message_by_name(schema.name.as_str()).unwrap()
}
