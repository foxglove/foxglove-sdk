use bytes::BytesMut;
use chrono::{TimeZone, Utc};
use foxglove::Encode;
use prost::Message;

#[derive(Encode)]
struct TestMessageWithTimestamp {
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Encode)]
struct TestMessageWithDuration {
    duration: chrono::TimeDelta,
}

#[derive(Encode)]
struct TestMessageWithBothTypes {
    created_at: chrono::DateTime<chrono::Utc>,
    elapsed: chrono::TimeDelta,
    value: u32,
}

#[test]
fn test_datetime_field_serialization() {
    let dt = Utc.with_ymd_and_hms(2024, 1, 15, 12, 30, 45).unwrap()
        + chrono::Duration::nanoseconds(123_456_789);

    let test_struct = TestMessageWithTimestamp { timestamp: dt };

    // Encode the struct
    let mut buf = BytesMut::with_capacity(test_struct.encoded_len().unwrap());
    test_struct.encode(&mut buf).expect("encode failed");

    // Verify the schema references google.protobuf.Timestamp
    let schema = TestMessageWithTimestamp::get_schema().expect("schema");
    assert_eq!(schema.encoding, "protobuf");

    let fds = prost_types::FileDescriptorSet::decode(schema.data.as_ref()).expect("decode schema");

    // The main message file is the last one (well-known type files come first)
    let file = fds.file.last().expect("at least one file");
    let message = &file.message_type[0];
    let field = &message.field[0];

    assert_eq!(field.name(), "timestamp");
    assert_eq!(field.type_name(), ".google.protobuf.Timestamp");

    // Verify the google.protobuf.Timestamp file is included
    assert!(
        fds.file
            .iter()
            .any(|f| f.package() == "google.protobuf"
                && f.name() == "google/protobuf/timestamp.proto")
    );
}

#[test]
fn test_timedelta_field_serialization() {
    let delta = chrono::TimeDelta::seconds(123) + chrono::TimeDelta::nanoseconds(456_789);

    let test_struct = TestMessageWithDuration { duration: delta };

    // Encode the struct
    let mut buf = BytesMut::with_capacity(test_struct.encoded_len().unwrap());
    test_struct.encode(&mut buf).expect("encode failed");

    // Verify the schema references google.protobuf.Duration
    let schema = TestMessageWithDuration::get_schema().expect("schema");
    let fds = prost_types::FileDescriptorSet::decode(schema.data.as_ref()).expect("decode schema");

    // The main message file is the last one (well-known type files come first)
    let file = fds.file.last().expect("at least one file");
    let message = &file.message_type[0];
    let field = &message.field[0];

    assert_eq!(field.name(), "duration");
    assert_eq!(field.type_name(), ".google.protobuf.Duration");

    // Verify the google.protobuf.Duration file is included
    assert!(fds
        .file
        .iter()
        .any(|f| f.package() == "google.protobuf" && f.name() == "google/protobuf/duration.proto"));
}

#[test]
fn test_mixed_chrono_fields_serialization() {
    let dt = Utc.with_ymd_and_hms(2024, 6, 15, 10, 0, 0).unwrap();
    let delta = chrono::TimeDelta::seconds(60);

    let test_struct = TestMessageWithBothTypes {
        created_at: dt,
        elapsed: delta,
        value: 42,
    };

    // Encode the struct
    let mut buf = BytesMut::with_capacity(test_struct.encoded_len().unwrap());
    test_struct.encode(&mut buf).expect("encode failed");

    // Verify the schema
    let schema = TestMessageWithBothTypes::get_schema().expect("schema");
    let fds = prost_types::FileDescriptorSet::decode(schema.data.as_ref()).expect("decode schema");

    // The main message file is the last one (well-known type files come first)
    let file = fds.file.last().expect("at least one file");
    let message = &file.message_type[0];

    assert_eq!(message.field.len(), 3);

    let created_at_field = &message.field[0];
    assert_eq!(created_at_field.name(), "created_at");
    assert_eq!(created_at_field.type_name(), ".google.protobuf.Timestamp");

    let elapsed_field = &message.field[1];
    assert_eq!(elapsed_field.name(), "elapsed");
    assert_eq!(elapsed_field.type_name(), ".google.protobuf.Duration");

    let value_field = &message.field[2];
    assert_eq!(value_field.name(), "value");
    // u32 maps to uint32, which doesn't have a type_name (it's a primitive)
    assert!(value_field.type_name.is_none() || value_field.type_name().is_empty());

    // Verify both well-known type files are included
    assert!(
        fds.file
            .iter()
            .any(|f| f.package() == "google.protobuf"
                && f.name() == "google/protobuf/timestamp.proto")
    );
    assert!(fds
        .file
        .iter()
        .any(|f| f.package() == "google.protobuf" && f.name() == "google/protobuf/duration.proto"));
}
