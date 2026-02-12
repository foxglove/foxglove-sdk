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

#[derive(Encode)]
struct TestMessageVecEnum {
    values: Vec<TestEnum>,
}

#[derive(Encode)]
struct TestMessageArrayEnum {
    values: [TestEnum; 3],
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
            .unwrap_or_else(|| panic!("Field '{field_name}' not found"));

        let field_value = deserialized_message.get_field(&field_descriptor);

        // Try to access the value as an enum number
        if let Some(value) = field_value.as_enum_number() {
            assert_eq!(value, expected_value);
        } else {
            panic!("Couldn't access field value as enum number");
        }
    }
}

#[test]
fn test_vec_of_enum_serialization() {
    let test_struct = TestMessageVecEnum {
        values: vec![TestEnum::ValueNeg, TestEnum::Value2, TestEnum::ValueOne],
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("Failed to encode");

    let schema = TestMessageVecEnum::get_schema().expect("Failed to get schema");
    assert_eq!(schema.encoding, "protobuf");

    let message_descriptor = get_message_descriptor(&schema);

    // Verify the field is marked as repeated
    let field_descriptor = message_descriptor
        .get_field_by_name("values")
        .expect("Field 'values' not found");
    assert!(
        field_descriptor.is_list(),
        "Field should be a repeated list"
    );

    let deserialized_message = DynamicMessage::decode(message_descriptor.clone(), buf.as_ref())
        .expect("Failed to deserialize");

    let field_value = deserialized_message.get_field(&field_descriptor);
    let list_value = field_value.as_list().expect("Field value is not a list");

    assert_eq!(list_value.len(), 3, "Vec should have 3 elements");

    let expected_values = [-10, 2, 1];
    for (i, expected) in expected_values.iter().enumerate() {
        let value = list_value[i]
            .as_enum_number()
            .expect("List item should be an enum");
        assert_eq!(value, *expected, "Enum value at index {} is wrong", i);
    }
}

#[test]
fn test_array_of_enum_serialization() {
    let test_struct = TestMessageArrayEnum {
        values: [TestEnum::ValueOne, TestEnum::ValueNeg, TestEnum::Value2],
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("Failed to encode");

    let schema = TestMessageArrayEnum::get_schema().expect("Failed to get schema");
    assert_eq!(schema.encoding, "protobuf");

    let message_descriptor = get_message_descriptor(&schema);

    // Verify the field is marked as repeated
    let field_descriptor = message_descriptor
        .get_field_by_name("values")
        .expect("Field 'values' not found");
    assert!(
        field_descriptor.is_list(),
        "Field should be a repeated list"
    );

    let deserialized_message = DynamicMessage::decode(message_descriptor.clone(), buf.as_ref())
        .expect("Failed to deserialize");

    let field_value = deserialized_message.get_field(&field_descriptor);
    let list_value = field_value.as_list().expect("Field value is not a list");

    assert_eq!(list_value.len(), 3, "Array should have 3 elements");

    let expected_values = [1, -10, 2];
    for (i, expected) in expected_values.iter().enumerate() {
        let value = list_value[i]
            .as_enum_number()
            .expect("List item should be an enum");
        assert_eq!(value, *expected, "Enum value at index {} is wrong", i);
    }
}

fn get_message_descriptor(schema: &Schema) -> MessageDescriptor {
    let descriptor_set = prost_types::FileDescriptorSet::decode(schema.data.as_ref())
        .expect("Failed to decode descriptor set");

    let pool = DescriptorPool::from_file_descriptor_set(descriptor_set).unwrap();

    pool.get_message_by_name(schema.name.as_str()).unwrap()
}
