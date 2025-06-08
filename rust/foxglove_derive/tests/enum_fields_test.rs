use ::foxglove::{Encode, Schema};
use bytes::BytesMut;
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor};

// Ensure the macro properly references the foxglove crate
mod foxglove {}

#[derive(Debug, Clone, Copy, Encode)]
enum TestEnum {
    ValueNeg = -10,
    ValueOne = 1,
    Value2 = 2,
}

#[derive(Encode)]
struct TestMessage {
    a: TestEnum,
    b: TestEnum,
    c: TestEnum,
}

#[test]
fn test_single_enum_field_serialization() {
    let test_struct = TestMessage {
        a: TestEnum::ValueNeg,
        b: TestEnum::Value2,
        c: TestEnum::ValueOne,
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("Failed to encode");

    let schema = TestMessage::get_schema().expect("Failed to get schema");
    assert_eq!(schema.encoding, "protobuf");

    assert_eq!(schema.name, "testmessage.TestMessage");

    let message_descriptor = get_message_descriptor(&schema);

    let deserialized_message = DynamicMessage::decode(message_descriptor.clone(), buf.as_ref())
        .expect("Failed to deserialize");

    let fields = [("a", -10), ("b", 2), ("c", 1)];
    for (field_name, expected_value) in fields {
        let field_descriptor = message_descriptor
            .get_field_by_name(field_name)
            .unwrap_or_else(|| panic!("Field '{}' not found", field_name));

        let field_value = deserialized_message.get_field(&field_descriptor);

        // Try to access the value as an enum number
        if let Some(value) = field_value.as_enum_number() {
            assert_eq!(value, expected_value);
        } else {
            panic!("Couldn't access field value as enum number");
        }
    }
}

fn get_message_descriptor(schema: &Schema) -> MessageDescriptor {
    let descriptor_set = prost_types::FileDescriptorSet::decode(schema.data.as_ref())
        .expect("Failed to decode descriptor set");

    let pool = DescriptorPool::from_file_descriptor_set(descriptor_set).unwrap();

    pool.get_message_by_name(schema.name.as_str()).unwrap()
}
