use ::foxglove::{Encode, Schema};
use bytes::BytesMut;
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor, ReflectMessage};

mod common;
use common::FixedSizeBuffer;

// Ensure the macro properly references the foxglove crate
mod foxglove {}

/// A newtype wrapping a primitive.
#[derive(Encode)]
struct MyU64(u64);

/// A newtype wrapping a String.
#[derive(Encode)]
struct MyString(String);

/// A newtype wrapping a nested struct.
#[derive(Encode)]
struct Inner {
    number: u64,
    name: String,
}

#[derive(Encode)]
struct WrappedInner(Inner);

/// A struct that uses newtype fields.
#[derive(Encode)]
struct MessageWithNewtypes {
    id: MyU64,
    label: MyString,
}

/// A struct that uses a newtype wrapping a struct.
#[derive(Encode)]
struct MessageWithWrappedStruct {
    inner: WrappedInner,
    extra: u32,
}

/// A struct with Vec of newtypes.
#[derive(Encode)]
struct MessageWithVecOfNewtypes {
    values: Vec<MyU64>,
}

/// A struct with an optional newtype.
#[derive(Encode)]
struct MessageWithOptionalNewtype {
    value: Option<MyU64>,
}

/// A generic newtype.
#[derive(Encode)]
struct Wrapper<T>(T);

/// A struct using the generic newtype.
#[derive(Encode)]
struct MessageWithGenericNewtype {
    value: Wrapper<u32>,
}

#[test]
fn test_primitive_newtype_field() {
    let msg = MessageWithNewtypes {
        id: MyU64(42),
        label: MyString("hello".to_string()),
    };

    let mut buf = BytesMut::new();
    msg.encode(&mut buf).expect("Failed to encode");

    let schema = MessageWithNewtypes::get_schema().expect("Failed to get schema");
    assert_eq!(schema.encoding, "protobuf");

    let message_descriptor = get_message_descriptor(&schema);
    let deserialized =
        DynamicMessage::decode(message_descriptor.clone(), buf.as_ref()).expect("Failed to decode");

    let id_field = message_descriptor
        .get_field_by_name("id")
        .expect("Field 'id' not found");
    assert_eq!(deserialized.get_field(&id_field).as_u64().unwrap(), 42);

    let label_field = message_descriptor
        .get_field_by_name("label")
        .expect("Field 'label' not found");
    assert_eq!(
        deserialized.get_field(&label_field).as_str().unwrap(),
        "hello"
    );
}

#[test]
fn test_struct_newtype_field() {
    let msg = MessageWithWrappedStruct {
        inner: WrappedInner(Inner {
            number: 99,
            name: "wrapped".to_string(),
        }),
        extra: 7,
    };

    let mut buf = BytesMut::new();
    msg.encode(&mut buf).expect("Failed to encode");

    let schema = MessageWithWrappedStruct::get_schema().expect("Failed to get schema");
    let message_descriptor = get_message_descriptor(&schema);
    let deserialized =
        DynamicMessage::decode(message_descriptor.clone(), buf.as_ref()).expect("Failed to decode");

    // Check the extra field
    let extra_field = message_descriptor
        .get_field_by_name("extra")
        .expect("Field 'extra' not found");
    assert_eq!(deserialized.get_field(&extra_field).as_u32().unwrap(), 7);

    // Check the inner message (newtype is transparent, so it encodes as Inner)
    let inner_field = message_descriptor
        .get_field_by_name("inner")
        .expect("Field 'inner' not found");
    let inner_msg = deserialized
        .get_field(&inner_field)
        .as_message()
        .expect("Expected a message")
        .clone();

    let number_field = inner_msg
        .descriptor()
        .get_field_by_name("number")
        .expect("Field 'number' not found");
    assert_eq!(inner_msg.get_field(&number_field).as_u64().unwrap(), 99);

    let name_field = inner_msg
        .descriptor()
        .get_field_by_name("name")
        .expect("Field 'name' not found");
    assert_eq!(
        inner_msg.get_field(&name_field).as_str().unwrap(),
        "wrapped"
    );
}

#[test]
fn test_vec_of_newtypes() {
    let msg = MessageWithVecOfNewtypes {
        values: vec![MyU64(10), MyU64(20), MyU64(30)],
    };

    let mut buf = BytesMut::new();
    msg.encode(&mut buf).expect("Failed to encode");

    let schema = MessageWithVecOfNewtypes::get_schema().expect("Failed to get schema");
    let message_descriptor = get_message_descriptor(&schema);
    let deserialized =
        DynamicMessage::decode(message_descriptor.clone(), buf.as_ref()).expect("Failed to decode");

    let values_field = message_descriptor
        .get_field_by_name("values")
        .expect("Field 'values' not found");
    assert!(values_field.is_list());

    let list = deserialized
        .get_field(&values_field)
        .as_list()
        .expect("Expected a list")
        .to_vec();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].as_u64().unwrap(), 10);
    assert_eq!(list[1].as_u64().unwrap(), 20);
    assert_eq!(list[2].as_u64().unwrap(), 30);
}

#[test]
fn test_optional_newtype_some() {
    let msg = MessageWithOptionalNewtype {
        value: Some(MyU64(42)),
    };

    let mut buf = BytesMut::new();
    msg.encode(&mut buf).expect("Failed to encode");

    let schema = MessageWithOptionalNewtype::get_schema().expect("Failed to get schema");
    let message_descriptor = get_message_descriptor(&schema);
    let deserialized =
        DynamicMessage::decode(message_descriptor.clone(), buf.as_ref()).expect("Failed to decode");

    let field = message_descriptor
        .get_field_by_name("value")
        .expect("Field 'value' not found");
    assert_eq!(deserialized.get_field(&field).as_u64().unwrap(), 42);
}

#[test]
fn test_optional_newtype_none() {
    let msg = MessageWithOptionalNewtype { value: None };

    let mut buf = BytesMut::new();
    msg.encode(&mut buf).expect("Failed to encode");

    let schema = MessageWithOptionalNewtype::get_schema().expect("Failed to get schema");
    let message_descriptor = get_message_descriptor(&schema);
    let deserialized =
        DynamicMessage::decode(message_descriptor.clone(), buf.as_ref()).expect("Failed to decode");

    let field = message_descriptor
        .get_field_by_name("value")
        .expect("Field 'value' not found");
    // Default value for u64 in proto3 is 0
    assert_eq!(deserialized.get_field(&field).as_u64().unwrap(), 0);
}

#[test]
fn test_newtype_encoded_len() {
    let msg = MessageWithNewtypes {
        id: MyU64(42),
        label: MyString("hello".to_string()),
    };

    let mut buf = BytesMut::new();
    msg.encode(&mut buf).expect("Failed to encode");

    let reported_len = msg.encoded_len().expect("encoded_len should return Some");
    assert_eq!(
        reported_len,
        buf.len(),
        "encoded_len() reported {} but actual encoded size is {}",
        reported_len,
        buf.len()
    );
}

#[test]
fn test_generic_newtype() {
    let msg = MessageWithGenericNewtype {
        value: Wrapper(123u32),
    };

    let mut buf = BytesMut::new();
    msg.encode(&mut buf).expect("Failed to encode");

    let schema = MessageWithGenericNewtype::get_schema().expect("Failed to get schema");
    let message_descriptor = get_message_descriptor(&schema);
    let deserialized =
        DynamicMessage::decode(message_descriptor.clone(), buf.as_ref()).expect("Failed to decode");

    let field = message_descriptor
        .get_field_by_name("value")
        .expect("Field 'value' not found");
    assert_eq!(deserialized.get_field(&field).as_u32().unwrap(), 123);
}

// ── Standalone newtype encode tests ──

#[test]
fn test_standalone_primitive_newtype() {
    let msg = MyU64(42);

    let mut buf = BytesMut::new();
    msg.encode(&mut buf).expect("Failed to encode");

    let schema = MyU64::get_schema().expect("Failed to get schema");
    assert_eq!(schema.encoding, "protobuf");
    assert_eq!(schema.name, "myu64.MyU64");

    let message_descriptor = get_message_descriptor(&schema);
    let deserialized =
        DynamicMessage::decode(message_descriptor.clone(), buf.as_ref()).expect("Failed to decode");

    let field = message_descriptor
        .get_field_by_name("value")
        .expect("Field 'value' not found");
    assert_eq!(deserialized.get_field(&field).as_u64().unwrap(), 42);
}

#[test]
fn test_standalone_string_newtype() {
    let msg = MyString("hello".to_string());

    let mut buf = BytesMut::new();
    msg.encode(&mut buf).expect("Failed to encode");

    let schema = MyString::get_schema().expect("Failed to get schema");
    let message_descriptor = get_message_descriptor(&schema);
    let deserialized =
        DynamicMessage::decode(message_descriptor.clone(), buf.as_ref()).expect("Failed to decode");

    let field = message_descriptor
        .get_field_by_name("value")
        .expect("Field 'value' not found");
    assert_eq!(deserialized.get_field(&field).as_str().unwrap(), "hello");
}

#[test]
fn test_standalone_struct_wrapping_newtype() {
    let msg = WrappedInner(Inner {
        number: 99,
        name: "wrapped".to_string(),
    });

    let mut buf = BytesMut::new();
    msg.encode(&mut buf).expect("Failed to encode");

    let schema = WrappedInner::get_schema().expect("Failed to get schema");
    let message_descriptor = get_message_descriptor(&schema);
    let deserialized =
        DynamicMessage::decode(message_descriptor.clone(), buf.as_ref()).expect("Failed to decode");

    let value_field = message_descriptor
        .get_field_by_name("value")
        .expect("Field 'value' not found");
    let inner_msg = deserialized
        .get_field(&value_field)
        .as_message()
        .expect("Expected a message")
        .clone();

    let number_field = inner_msg
        .descriptor()
        .get_field_by_name("number")
        .expect("Field 'number' not found");
    assert_eq!(inner_msg.get_field(&number_field).as_u64().unwrap(), 99);

    let name_field = inner_msg
        .descriptor()
        .get_field_by_name("name")
        .expect("Field 'name' not found");
    assert_eq!(
        inner_msg.get_field(&name_field).as_str().unwrap(),
        "wrapped"
    );
}

#[test]
fn test_standalone_generic_newtype() {
    let msg = Wrapper(123u32);

    let mut buf = BytesMut::new();
    msg.encode(&mut buf).expect("Failed to encode");

    let schema = Wrapper::<u32>::get_schema().expect("Failed to get schema");
    let message_descriptor = get_message_descriptor(&schema);
    let deserialized =
        DynamicMessage::decode(message_descriptor.clone(), buf.as_ref()).expect("Failed to decode");

    let field = message_descriptor
        .get_field_by_name("value")
        .expect("Field 'value' not found");
    assert_eq!(deserialized.get_field(&field).as_u32().unwrap(), 123);
}

#[test]
fn test_newtype_schema_structure() {
    let schema = MyU64::get_schema().expect("Failed to get schema");
    let descriptor_set = prost_types::FileDescriptorSet::decode(schema.data.as_ref())
        .expect("Failed to decode descriptor set");

    // Find the file that contains our message (last file in the set)
    let file = descriptor_set.file.last().expect("No file descriptors");
    assert_eq!(file.name.as_deref(), Some("MyU64.proto"));
    assert_eq!(file.package.as_deref(), Some("myu64"));

    let message = &file.message_type[0];
    assert_eq!(message.name.as_deref(), Some("MyU64"));
    assert_eq!(message.field.len(), 1);

    let field = &message.field[0];
    assert_eq!(field.name.as_deref(), Some("value"));
    assert_eq!(field.number, Some(1));
}

#[test]
fn test_standalone_newtype_encoded_len() {
    let msg = MyU64(42);

    let mut buf = BytesMut::new();
    msg.encode(&mut buf).expect("Failed to encode");

    let reported_len = msg.encoded_len().expect("encoded_len should return Some");
    assert_eq!(
        reported_len,
        buf.len(),
        "encoded_len() reported {} but actual encoded size is {}",
        reported_len,
        buf.len()
    );
}

#[test]
fn test_standalone_newtype_buffer_overflow() {
    let msg = MyU64(42);
    let mut buf = FixedSizeBuffer::with_capacity(1);
    let result = msg.encode(&mut buf);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "Encoding error: insufficient buffer"
    );
}

fn get_message_descriptor(schema: &Schema) -> MessageDescriptor {
    let descriptor_set = prost_types::FileDescriptorSet::decode(schema.data.as_ref())
        .expect("Failed to decode descriptor set");

    let pool = DescriptorPool::from_file_descriptor_set(descriptor_set).unwrap();

    pool.get_message_by_name(schema.name.as_str())
        .unwrap_or_else(|| panic!("Failed to get message descriptor for {}", schema.name))
}
