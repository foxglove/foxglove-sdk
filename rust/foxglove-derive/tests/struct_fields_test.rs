use ::foxglove::{Encode, Schema};
use bytes::BytesMut;
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor, ReflectMessage};

// Ensure the macro properly references the foxglove crate
mod foxglove {}

#[derive(Encode, Debug)]
struct InnerTestMessage {
    number: u64,
    name: String,
}

#[derive(Encode, Debug)]
struct MiddleTestMessage {
    last: InnerTestMessage,
    description: String,
}

#[derive(Encode, Debug)]
struct NestedTestMessage {
    middle: MiddleTestMessage,
    id: u32,
}

#[derive(Encode, Debug)]
struct TestMessage {
    inner: InnerTestMessage,
}

#[derive(Encode, Debug)]
struct TestMessageVectorOfStructs {
    items: Vec<InnerTestMessage>,
}

#[derive(Encode, Debug)]
struct RepeatedTestMessage {
    a: InnerTestMessage,
    b: InnerTestMessage,
}

#[test]
fn test_struct_field_serialization() {
    let test_struct = TestMessage {
        inner: InnerTestMessage {
            number: 42,
            name: "foo".to_string(),
        },
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("Failed to encode");

    let schema = TestMessage::get_schema().expect("Failed to get schema");
    assert_eq!(schema.encoding, "protobuf");
    assert_eq!(schema.name, "testmessage.TestMessage");

    let message_descriptor = get_message_descriptor(&schema);

    let deserialized_message = DynamicMessage::decode(message_descriptor.clone(), buf.as_ref())
        .expect("Failed to deserialize");

    println!("Message: {:#?}", deserialized_message);

    // Get the inner field descriptor
    let inner_field_desc = message_descriptor
        .get_field_by_name("inner")
        .expect("Field 'inner' not found");

    // Verify the field has correct MessageKind type
    println!("Inner field kind: {:?}", inner_field_desc.kind());

    // Get the inner message value
    let inner_value = deserialized_message.get_field(&inner_field_desc);
    println!("Inner field value: {:?}", inner_value);

    // Get and verify the inner message
    let inner_message = inner_value.as_message().expect("Expected a message");

    // Get inner message descriptor
    let inner_descriptor = inner_message.descriptor();

    // Verify foo field
    let number_field = inner_descriptor
        .get_field_by_name("number")
        .expect("Field 'number' not found in inner message");
    let number_value = inner_message.get_field(&number_field);
    assert_eq!(
        number_value.as_u64().unwrap(),
        42,
        "Inner number field has wrong value"
    );

    // Verify bar field
    let name_field = inner_descriptor
        .get_field_by_name("name")
        .expect("Field 'name' not found in inner message");
    let name_value = inner_message.get_field(&name_field);
    assert_eq!(
        name_value.as_str().unwrap(),
        "foo",
        "Inner name field has wrong value"
    );
}

#[test]
fn test_vector_of_structs_serialization() {
    // Create a vector of InnerTestMessage instances
    let test_struct = TestMessageVectorOfStructs {
        items: vec![
            InnerTestMessage {
                number: 42,
                name: "first".to_string(),
            },
            InnerTestMessage {
                number: 84,
                name: "second".to_string(),
            },
            InnerTestMessage {
                number: 126,
                name: "third".to_string(),
            },
        ],
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("Failed to encode");

    let schema = TestMessageVectorOfStructs::get_schema().expect("Failed to get schema");
    assert_eq!(schema.encoding, "protobuf");
    assert_eq!(
        schema.name,
        "testmessagevectorofstructs.TestMessageVectorOfStructs"
    );

    let descriptor_set = prost_types::FileDescriptorSet::decode(schema.data.as_ref())
        .expect("Failed to decode descriptor set");
    let file = &descriptor_set.file[0];

    // Find the main message
    let message_type = file
        .message_type
        .iter()
        .find(|m| m.name.as_ref().unwrap() == "TestMessageVectorOfStructs")
        .expect("TestMessageVectorOfStructs descriptor missing");

    // Verify the items field is marked as repeated
    let field = &message_type.field[0];
    assert_eq!(field.name.as_ref().unwrap(), "items");
    assert_eq!(
        field.label.unwrap(),
        prost_types::field_descriptor_proto::Label::Repeated as i32,
        "Field should be marked as repeated"
    );
    assert_eq!(
        field.r#type.unwrap(),
        prost_types::field_descriptor_proto::Type::Message as i32,
        "Field should be of message type"
    );

    // The type_name should reference the InnerTestMessage
    assert!(
        field
            .type_name
            .as_ref()
            .unwrap()
            .contains("InnerTestMessage"),
        "Field type_name should reference InnerTestMessage"
    );

    // Print binary data for debugging
    println!("Encoded vector of structs binary: {:x?}", buf.as_ref());

    // Deserialize and verify
    let message_descriptor = get_message_descriptor(&schema);
    let deserialized_message = DynamicMessage::decode(message_descriptor.clone(), buf.as_ref())
        .expect("Failed to deserialize vector message");

    // Get the items field descriptor
    let items_field_desc = message_descriptor
        .get_field_by_name("items")
        .expect("Field 'items' not found");

    assert_eq!(items_field_desc.name(), "items");
    assert!(
        items_field_desc.is_list(),
        "Field should be a repeated list"
    );

    // Get the list value and verify each element
    let field_value = deserialized_message.get_field(&items_field_desc);
    let list_value = field_value.as_list().expect("Field value is not a list");

    assert_eq!(list_value.len(), 3, "Vector should have 3 elements");

    // Check each InnerTestMessage in the list
    for (i, item) in list_value.iter().enumerate() {
        let item_message = item.as_message().expect("List item should be a message");

        // Check number field
        let number_field = item_message
            .descriptor()
            .get_field_by_name("number")
            .expect("Field 'number' not found in list item");
        let number_value = item_message.get_field(&number_field);

        // Check name field
        let name_field = item_message
            .descriptor()
            .get_field_by_name("name")
            .expect("Field 'name' not found in list item");
        let name_value = item_message.get_field(&name_field);

        match i {
            0 => {
                assert_eq!(
                    number_value.as_u64().unwrap(),
                    42,
                    "First item number wrong"
                );
                assert_eq!(
                    name_value.as_str().unwrap(),
                    "first",
                    "First item name wrong"
                );
            }
            1 => {
                assert_eq!(
                    number_value.as_u64().unwrap(),
                    84,
                    "Second item number wrong"
                );
                assert_eq!(
                    name_value.as_str().unwrap(),
                    "second",
                    "Second item name wrong"
                );
            }
            2 => {
                assert_eq!(
                    number_value.as_u64().unwrap(),
                    126,
                    "Third item number wrong"
                );
                assert_eq!(
                    name_value.as_str().unwrap(),
                    "third",
                    "Third item name wrong"
                );
            }
            _ => panic!("Unexpected item index"),
        }
    }
}

#[test]
fn test_nested_struct_serialization() {
    let test_struct = NestedTestMessage {
        middle: MiddleTestMessage {
            last: InnerTestMessage {
                number: 42,
                name: "foo".to_string(),
            },
            description: "middle layer".to_string(),
        },
        id: 123,
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("Failed to encode");

    let schema = NestedTestMessage::get_schema().expect("Failed to get schema");
    assert_eq!(schema.encoding, "protobuf");
    assert_eq!(schema.name, "nestedtestmessage.NestedTestMessage");

    let message_descriptor = get_message_descriptor(&schema);

    let deserialized_message = DynamicMessage::decode(message_descriptor.clone(), buf.as_ref())
        .expect("Failed to deserialize");

    // Check top-level fields
    let id_field = message_descriptor
        .get_field_by_name("id")
        .expect("Field 'id' not found");
    let id_value = deserialized_message.get_field(&id_field);
    assert_eq!(id_value.as_u32().unwrap(), 123, "ID field has wrong value");

    // Get the middle field
    let middle_field_desc = message_descriptor
        .get_field_by_name("middle")
        .expect("Field 'middle' not found");
    let middle_value = deserialized_message.get_field(&middle_field_desc);
    let middle_message = middle_value.as_message().expect("Expected a message");
    let middle_descriptor = middle_message.descriptor();

    // Check middle layer fields
    let desc_field = middle_descriptor
        .get_field_by_name("description")
        .expect("Field 'description' not found");
    let desc_value = middle_message.get_field(&desc_field);
    assert_eq!(
        desc_value.as_str().unwrap(),
        "middle layer",
        "Description field has wrong value"
    );

    // Get the inner field
    let last_field_desc = middle_descriptor
        .get_field_by_name("last")
        .expect("Field 'last' not found");
    let last_value = middle_message.get_field(&last_field_desc);
    let last_message = last_value.as_message().expect("Expected a message");
    let last_descriptor = last_message.descriptor();

    // Check innermost fields
    let number_field = last_descriptor
        .get_field_by_name("number")
        .expect("Field 'number' not found");
    let number_value = last_message.get_field(&number_field);
    assert_eq!(
        number_value.as_u64().unwrap(),
        42,
        "Number field has wrong value"
    );

    let name_field = last_descriptor
        .get_field_by_name("name")
        .expect("Field 'name' not found");
    let name_value = last_message.get_field(&name_field);
    assert_eq!(
        name_value.as_str().unwrap(),
        "foo",
        "Name field has wrong value"
    );
}

#[test]
fn test_repeated_struct_serialization() {
    let repeated_msg = RepeatedTestMessage {
        a: InnerTestMessage {
            number: 42,
            name: "foo".to_string(),
        },
        b: InnerTestMessage {
            number: 43,
            name: "bar".to_string(),
        },
    };
    let mut buf = BytesMut::new();
    repeated_msg.encode(&mut buf).expect("Failed to encode");

    let schema = RepeatedTestMessage::get_schema().expect("Failed to get schema");
    assert_eq!(schema.encoding, "protobuf");
    assert_eq!(schema.name, "repeatedtestmessage.RepeatedTestMessage");

    let message_descriptor = get_message_descriptor(&schema);

    let deserialized_message = DynamicMessage::decode(message_descriptor.clone(), buf.as_ref())
        .expect("Failed to deserialize");

    let fields = [("a", 42), ("b", 43)];
    for (field_name, expected_value) in fields {
        let field = message_descriptor
            .get_field_by_name(field_name)
            .unwrap_or_else(|| panic!("Field '{}' not found", field_name));

        let field = deserialized_message.get_field(&field);
        let inner_msg = field.as_message().expect("Expected a message");

        let desc_field = inner_msg
            .descriptor()
            .get_field_by_name("number")
            .expect("Field 'number' not found");
        let desc_value = inner_msg.get_field(&desc_field);
        assert_eq!(
            desc_value.as_u64().unwrap(),
            expected_value,
            "Number field has wrong value"
        );
    }
}

fn get_message_descriptor(schema: &Schema) -> MessageDescriptor {
    let descriptor_set = prost_types::FileDescriptorSet::decode(schema.data.as_ref())
        .expect("Failed to decode descriptor set");

    let pool = DescriptorPool::from_file_descriptor_set(descriptor_set).unwrap();

    pool.get_message_by_name(schema.name.as_str()).unwrap()
}
