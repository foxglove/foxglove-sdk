use bytes::BytesMut;
use chrono::{TimeZone, Utc};
use foxglove::Encode;
use prost::Message;
use prost_reflect::DescriptorPool;

#[derive(Encode)]
struct TestMessageWithTimestamp {
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Encode)]
struct OuterWithNestedTimestamp {
    inner: TestMessageWithTimestamp,
    value: u32,
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

/// Struct with direct WKT field AND nested struct that also uses the same WKT.
/// Tests that we don't get duplicate file descriptors.
#[derive(Encode)]
struct OuterWithDirectAndNestedTimestamp {
    direct_ts: chrono::DateTime<chrono::Utc>,
    inner: TestMessageWithTimestamp,
}

/// Deeply nested: Outer -> Middle -> Inner (with WKT)
#[derive(Encode)]
struct MiddleStruct {
    inner: TestMessageWithTimestamp,
    count: u32,
}

#[derive(Encode)]
struct DeeplyNestedWkt {
    middle: MiddleStruct,
    name: String,
}

/// Optional WKT field
#[derive(Encode)]
struct OptionalTimestamp {
    maybe_ts: Option<chrono::DateTime<chrono::Utc>>,
}

/// Vec of WKT
#[derive(Encode)]
struct VecOfTimestamps {
    timestamps: Vec<chrono::DateTime<chrono::Utc>>,
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
    assert!(
        fds.file
            .iter()
            .any(|f| f.package() == "google.protobuf"
                && f.name() == "google/protobuf/duration.proto")
    );
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
    assert!(
        fds.file
            .iter()
            .any(|f| f.package() == "google.protobuf"
                && f.name() == "google/protobuf/duration.proto")
    );
}

#[test]
fn test_nested_struct_with_wkt() {
    // This tests that when a derived struct contains another derived struct
    // that uses a WKT, the outer struct's schema includes the WKT file descriptor.
    let dt = Utc.with_ymd_and_hms(2024, 1, 15, 12, 30, 45).unwrap();
    let test_struct = OuterWithNestedTimestamp {
        inner: TestMessageWithTimestamp { timestamp: dt },
        value: 42,
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("encode failed");

    let schema = OuterWithNestedTimestamp::get_schema().expect("schema");
    let fds = prost_types::FileDescriptorSet::decode(schema.data.as_ref()).expect("decode schema");

    // The schema should include google.protobuf.Timestamp even though it's
    // referenced indirectly through the nested TestMessageWithTimestamp.
    assert!(
        fds.file
            .iter()
            .any(|f| f.package() == "google.protobuf"
                && f.name() == "google/protobuf/timestamp.proto"),
        "WKT file descriptor should be included for nested types"
    );

    // The schema should be parseable by DescriptorPool
    let pool = DescriptorPool::from_file_descriptor_set(fds)
        .expect("schema should be valid and parseable");

    // We should be able to find the outer message
    assert!(
        pool.get_message_by_name(&schema.name).is_some(),
        "outer message should be in the pool"
    );
}

#[test]
fn test_direct_and_nested_wkt_no_duplicates() {
    // This tests that when a struct has both a direct WKT field AND a nested struct
    // that uses the same WKT, we don't get duplicate file descriptors.
    let dt = Utc.with_ymd_and_hms(2024, 1, 15, 12, 30, 45).unwrap();
    let test_struct = OuterWithDirectAndNestedTimestamp {
        direct_ts: dt,
        inner: TestMessageWithTimestamp { timestamp: dt },
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("encode failed");

    let schema = OuterWithDirectAndNestedTimestamp::get_schema().expect("schema");
    let fds = prost_types::FileDescriptorSet::decode(schema.data.as_ref()).expect("decode schema");

    // Count how many times timestamp.proto appears
    let timestamp_count = fds
        .file
        .iter()
        .filter(|f| f.name() == "google/protobuf/timestamp.proto")
        .count();

    assert_eq!(
        timestamp_count, 1,
        "timestamp.proto should appear exactly once, not duplicated"
    );

    // The schema should still be parseable
    let pool = DescriptorPool::from_file_descriptor_set(fds)
        .expect("schema should be valid and parseable");

    assert!(
        pool.get_message_by_name(&schema.name).is_some(),
        "message should be in the pool"
    );
}

#[test]
fn test_deeply_nested_wkt() {
    // Tests WKT propagation through multiple levels of nesting:
    // DeeplyNestedWkt -> MiddleStruct -> TestMessageWithTimestamp -> DateTime<Utc>
    let dt = Utc.with_ymd_and_hms(2024, 1, 15, 12, 30, 45).unwrap();
    let test_struct = DeeplyNestedWkt {
        middle: MiddleStruct {
            inner: TestMessageWithTimestamp { timestamp: dt },
            count: 42,
        },
        name: "test".to_string(),
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("encode failed");

    let schema = DeeplyNestedWkt::get_schema().expect("schema");
    let fds = prost_types::FileDescriptorSet::decode(schema.data.as_ref()).expect("decode schema");

    // The WKT should be included even through deep nesting
    assert!(
        fds.file
            .iter()
            .any(|f| f.name() == "google/protobuf/timestamp.proto"),
        "WKT should be included through deep nesting"
    );

    // Should be parseable
    let pool = DescriptorPool::from_file_descriptor_set(fds)
        .expect("schema should be valid and parseable");

    assert!(
        pool.get_message_by_name(&schema.name).is_some(),
        "message should be in the pool"
    );
}

#[test]
fn test_optional_wkt_field() {
    // Tests that Option<WKT> properly includes the WKT file descriptor
    let dt = Utc.with_ymd_and_hms(2024, 1, 15, 12, 30, 45).unwrap();
    let test_struct = OptionalTimestamp { maybe_ts: Some(dt) };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("encode failed");

    let schema = OptionalTimestamp::get_schema().expect("schema");
    let fds = prost_types::FileDescriptorSet::decode(schema.data.as_ref()).expect("decode schema");

    assert!(
        fds.file
            .iter()
            .any(|f| f.name() == "google/protobuf/timestamp.proto"),
        "WKT should be included for Option<DateTime>"
    );

    let pool = DescriptorPool::from_file_descriptor_set(fds)
        .expect("schema should be valid and parseable");

    assert!(
        pool.get_message_by_name(&schema.name).is_some(),
        "message should be in the pool"
    );
}

#[test]
fn test_vec_of_wkt_field() {
    // Tests that Vec<WKT> properly includes the WKT file descriptor
    let dt1 = Utc.with_ymd_and_hms(2024, 1, 15, 12, 30, 45).unwrap();
    let dt2 = Utc.with_ymd_and_hms(2024, 1, 16, 12, 30, 45).unwrap();
    let test_struct = VecOfTimestamps {
        timestamps: vec![dt1, dt2],
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("encode failed");

    let schema = VecOfTimestamps::get_schema().expect("schema");
    let fds = prost_types::FileDescriptorSet::decode(schema.data.as_ref()).expect("decode schema");

    assert!(
        fds.file
            .iter()
            .any(|f| f.name() == "google/protobuf/timestamp.proto"),
        "WKT should be included for Vec<DateTime>"
    );

    let pool = DescriptorPool::from_file_descriptor_set(fds)
        .expect("schema should be valid and parseable");

    assert!(
        pool.get_message_by_name(&schema.name).is_some(),
        "message should be in the pool"
    );
}
