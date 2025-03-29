use bytes::BytesMut;
use foxglove::{Encode, Schema};
use foxglove_derive::Loggable;
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor};

#[derive(Debug, Clone, Copy, Loggable)]
enum TestEnum {
    #[allow(dead_code)]
    ValueOne,
    Value2,
}

#[derive(Loggable)]
struct TestMessage {
    val: TestEnum,
}

#[test]
fn test_single_enum_field_serialization() {
    let test_struct = TestMessage {
        val: TestEnum::Value2,
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
        .get_field_by_name("val")
        .expect("Field 'val' not found");
    assert_eq!(field_descriptor.name(), "val");

    let field_value = deserialized_message.get_field(&field_descriptor);
    println!("Field value type: {:?}", field_value);

    // Try to access the value as an enum number
    if let Some(value) = field_value.as_enum_number() {
        println!("Value as enum number: {}", value);
        assert_eq!(value, 1); // MessageLevel::Info should be encoded as 1
    } else if let Some(value) = field_value.as_i32() {
        println!("Value as i32: {}", value);
        assert_eq!(value, 1); // MessageLevel::Info should be encoded as 1
    } else {
        panic!("Couldn't access field value as enum number or i32");
    }
}

fn get_message_descriptor(schema: &Schema) -> MessageDescriptor {
    let descriptor_set = prost_types::FileDescriptorSet::decode(schema.data.as_ref())
        .expect("Failed to decode descriptor set");

    let pool = DescriptorPool::from_file_descriptor_set(descriptor_set).unwrap();

    pool.get_message_by_name(schema.name.as_str()).unwrap()
}
