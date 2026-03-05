//! Tests for the `messages` module and backward-compatible `schemas` module alias.
//!
//! These tests exercise the public API surface for message types, encoding/decoding traits,
//! protobuf field metadata, serde serialization, feature-gated functionality, and the
//! backward-compatible `schemas` module alias. They serve as a contract test suite ensuring
//! backward compatibility.

use crate::encode::Encode;
use crate::messages::GeoJson;

#[test]
fn test_geojson_schema_preserves_schema_name() {
    let schema = GeoJson::get_schema();
    assert!(schema.is_some());
    assert_eq!(schema.unwrap().name, "foxglove.GeoJSON");
}

#[test]
fn test_log_message_can_be_encoded() {
    use crate::messages::{Log, Timestamp, log::Level};

    let msg = Log {
        timestamp: Some(Timestamp::new(5, 10)),
        level: Level::Error as i32,
        message: "hello".to_string(),
        name: "logger".to_string(),
        file: "file".to_string(),
        line: 123,
    };

    let schema = Log::get_schema();
    assert!(schema.is_some());
    assert_eq!(schema.unwrap().name, "foxglove.Log");

    let mut buf = Vec::new();
    msg.encode(&mut buf).expect("encoding should succeed");
    assert!(!buf.is_empty());
}

#[test]
fn test_timestamp_creation() {
    use crate::messages::Timestamp;

    let ts = Timestamp::new(123, 456);
    assert_eq!(ts.sec(), 123);
    assert_eq!(ts.nsec(), 456);
}

/// Test that the deprecated `schemas` module re-exports the same types as `messages`.
///
/// Note: In Rust, we can't easily test that a deprecation warning is emitted at runtime.
/// The `#[deprecated]` attribute emits warnings at compile time. This test verifies that
/// the re-exported types are identical to those in the `messages` module.
#[test]
#[allow(deprecated)]
fn test_schemas_reexports_same_types_as_messages() {
    use crate::messages;
    use crate::schemas;

    // Verify that types from both modules are the same by checking type equality.
    // We create instances from both modules and verify they're compatible.
    let msg_from_messages: messages::Log = messages::Log {
        timestamp: Some(messages::Timestamp::new(1, 2)),
        level: messages::log::Level::Info as i32,
        message: "test".to_string(),
        ..Default::default()
    };

    let msg_from_schemas: schemas::Log = schemas::Log {
        timestamp: Some(schemas::Timestamp::new(1, 2)),
        level: schemas::log::Level::Info as i32,
        message: "test".to_string(),
        ..Default::default()
    };

    // Both should encode to the same bytes.
    let mut buf1 = Vec::new();
    let mut buf2 = Vec::new();
    msg_from_messages.encode(&mut buf1).unwrap();
    msg_from_schemas.encode(&mut buf2).unwrap();
    assert_eq!(buf1, buf2);

    // Verify schema names are identical.
    assert_eq!(
        messages::Log::get_schema().unwrap().name,
        schemas::Log::get_schema().unwrap().name
    );
}

/// Test that other common types are re-exported correctly from the deprecated schemas module.
#[test]
#[allow(deprecated)]
fn test_schemas_reexports_common_types() {
    use crate::messages;
    use crate::schemas;
    use std::any::TypeId;

    // Verify type identity using TypeId.
    assert_eq!(TypeId::of::<messages::Log>(), TypeId::of::<schemas::Log>());
    assert_eq!(
        TypeId::of::<messages::Timestamp>(),
        TypeId::of::<schemas::Timestamp>()
    );
    assert_eq!(
        TypeId::of::<messages::Duration>(),
        TypeId::of::<schemas::Duration>()
    );
    assert_eq!(
        TypeId::of::<messages::CompressedImage>(),
        TypeId::of::<schemas::CompressedImage>()
    );
    assert_eq!(
        TypeId::of::<messages::SceneUpdate>(),
        TypeId::of::<schemas::SceneUpdate>()
    );
    assert_eq!(
        TypeId::of::<messages::PointCloud>(),
        TypeId::of::<schemas::PointCloud>()
    );
}

/// Test that using the deprecated module with `use foxglove::schemas::*` works.
#[test]
#[allow(deprecated)]
fn test_schemas_glob_import_works() {
    #[allow(unused_imports)]
    use crate::schemas::*;

    // Create a message using types from glob import.
    let _ts = Timestamp::new(100, 200);
    let _color = Color {
        r: 1.0,
        g: 0.5,
        b: 0.0,
        a: 1.0,
    };
}

/// Test that `Encode`/`Decode` roundtrip produces the original message.
#[test]
fn test_encode_decode_roundtrip() {
    use crate::decode::Decode;
    use crate::messages::{Log, Timestamp, log::Level};

    let original = Log {
        timestamp: Some(Timestamp::new(1_700_000_000, 999_999_999)),
        level: Level::Warning as i32,
        message: "roundtrip test".to_string(),
        name: "test_node".to_string(),
        file: "test.rs".to_string(),
        line: 42,
    };

    let mut buf = Vec::new();
    original.encode(&mut buf).expect("encoding should succeed");

    let decoded = Log::decode(buf.as_slice()).expect("decoding should succeed");
    assert_eq!(original, decoded);
}

/// Test `ProtobufField` trait for a generated message type.
#[cfg(feature = "derive")]
#[test]
fn test_protobuf_field_for_message_type() {
    use crate::messages::Log;
    use crate::protobuf::ProtobufField;
    use prost_types::field_descriptor_proto::Type as ProstFieldType;

    // Generated message types should report as Message type with LengthDelimited wire type.
    assert_eq!(Log::field_type(), ProstFieldType::Message);
    assert_eq!(
        Log::wire_type(),
        prost::encoding::WireType::LengthDelimited as u32
    );

    // Should have a type name matching the protobuf fully-qualified name.
    assert_eq!(Log::type_name().as_deref(), Some(".foxglove.Log"));

    // Should provide file descriptors for schema exchange.
    assert!(!Log::file_descriptors().is_empty());
}

/// Test `ProtobufField` trait for the well-known `Timestamp` type.
#[cfg(feature = "derive")]
#[test]
fn test_protobuf_field_for_timestamp() {
    use crate::messages::Timestamp;
    use crate::protobuf::ProtobufField;
    use prost_types::field_descriptor_proto::Type as ProstFieldType;

    assert_eq!(Timestamp::field_type(), ProstFieldType::Message);
    assert_eq!(
        Timestamp::wire_type(),
        prost::encoding::WireType::LengthDelimited as u32
    );
    assert_eq!(
        Timestamp::type_name().as_deref(),
        Some(".google.protobuf.Timestamp")
    );

    // Should have a file descriptor for the google.protobuf.Timestamp type.
    let fd = Timestamp::file_descriptor().expect("Timestamp should have a file descriptor");
    assert_eq!(fd.name(), "google/protobuf/timestamp.proto");
}

/// Test `ProtobufField` write/read roundtrip for a generated message type.
#[cfg(feature = "derive")]
#[test]
fn test_protobuf_field_write_roundtrip() {
    use crate::messages::{Log, Timestamp, log::Level};
    use crate::protobuf::ProtobufField;

    let msg = Log {
        timestamp: Some(Timestamp::new(100, 200)),
        level: Level::Info as i32,
        message: "hello".to_string(),
        ..Default::default()
    };

    let mut buf = Vec::new();
    msg.write(&mut buf);

    // ProtobufField::encoded_len should match the actual written bytes.
    assert_eq!(ProtobufField::encoded_len(&msg), buf.len());

    // ProtobufField::write produces length-delimited format. Decode accordingly.
    let decoded = <Log as prost::Message>::decode_length_delimited(buf.as_slice())
        .expect("should decode length-delimited written bytes");
    assert_eq!(msg, decoded);
}

/// Test serde JSON roundtrip for a message with enum fields, verifying string enum names.
#[cfg(feature = "serde")]
#[test]
fn test_log_json_roundtrip_with_enum_strings() {
    use crate::messages::{Log, Timestamp, log::Level};

    let msg = Log {
        timestamp: Some(Timestamp::new(1_000_000, 500)),
        level: Level::Error as i32,
        message: "something went wrong".to_string(),
        name: "my_node".to_string(),
        file: "main.rs".to_string(),
        line: 99,
    };

    let json = serde_json::to_string(&msg).expect("serialization should succeed");

    // Enum should serialize as a string name in JSON (human-readable format).
    assert!(
        json.contains("\"ERROR\""),
        "enum should serialize as string name, got: {json}"
    );

    let parsed: Log = serde_json::from_str(&json).expect("deserialization should succeed");
    assert_eq!(msg, parsed);
}

/// Test serde JSON roundtrip for a message with bytes fields, verifying base64 encoding.
#[cfg(feature = "serde")]
#[test]
fn test_compressed_image_json_roundtrip_with_base64() {
    use bytes::Bytes;

    use crate::messages::{CompressedImage, Timestamp};

    let image_data = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03];
    let msg = CompressedImage {
        timestamp: Some(Timestamp::new(42, 0)),
        frame_id: "camera".to_string(),
        data: Bytes::from(image_data),
        format: "png".to_string(),
    };

    let json = serde_json::to_string(&msg).expect("serialization should succeed");

    // Bytes should serialize as base64 in JSON.
    // 0xDEADBEEF010203 -> base64 "3q2+7wECAw=="
    assert!(
        json.contains("3q2+7wECAw=="),
        "bytes should serialize as base64, got: {json}"
    );

    let parsed: CompressedImage =
        serde_json::from_str(&json).expect("deserialization should succeed");
    assert_eq!(msg, parsed);
}

/// Test chrono DateTime<Utc> to Timestamp conversion and ProtobufField write roundtrip.
#[cfg(all(feature = "chrono", feature = "derive"))]
#[test]
fn test_chrono_datetime_to_timestamp_protobuf_roundtrip() {
    use chrono::{TimeZone, Utc};

    use crate::messages::Timestamp;
    use crate::protobuf::ProtobufField;

    let expected_sec: i64 = 1_700_000_000;
    let expected_nsec: u32 = 123_456_789;

    let dt = Utc.timestamp_opt(expected_sec, expected_nsec).unwrap();
    let ts = Timestamp::try_from(dt).expect("conversion should succeed");

    assert_eq!(ts.sec() as i64, expected_sec);
    assert_eq!(ts.nsec(), expected_nsec);

    // Write via ProtobufField (produces length-delimited format) and decode back.
    let mut buf = Vec::new();
    ts.write(&mut buf);

    let decoded =
        <prost_types::Timestamp as prost::Message>::decode_length_delimited(buf.as_slice())
            .expect("should decode as prost Timestamp");
    assert_eq!(decoded.seconds, expected_sec);
    assert_eq!(decoded.nanos, expected_nsec as i32);
}

/// Test that the `convert` module's `SaturatingFrom` trait works for Timestamp edge cases.
#[cfg(feature = "chrono")]
#[test]
fn test_chrono_saturating_timestamp_conversion() {
    use chrono::{TimeZone, Utc};

    use crate::convert::SaturatingFrom;
    use crate::messages::Timestamp;

    // A value exceeding u32::MAX should saturate to Timestamp::MAX.
    let beyond_u32 = Utc.timestamp_opt(u32::MAX as i64 + 1, 0).unwrap();
    let ts = Timestamp::saturating_from(beyond_u32);
    assert_eq!(ts, Timestamp::MAX);

    // A negative timestamp should saturate to Timestamp::MIN.
    let before_epoch = Utc.timestamp_opt(-1, 0).unwrap();
    let ts = Timestamp::saturating_from(before_epoch);
    assert_eq!(ts, Timestamp::MIN);
}
