//! Tests for the `messages` module and backward-compatible `schemas` module alias.
//!
//! These tests exercise the public API surface for message types, encoding/decoding traits,
//! protobuf field metadata, serde serialization, feature-gated functionality, and the
//! backward-compatible `schemas` module alias. They serve as a contract test suite ensuring
//! backward compatibility.

use crate::encode::Encode;
use crate::messages::GeoJson;

/// Catches: broken `crate::messages` re-export path, broken `Encode::get_schema()` or `Schema`
/// struct for a type with non-standard casing (GeoJSON).
#[test]
fn test_geojson_schema_preserves_schema_name() {
    let schema = GeoJson::get_schema();
    assert!(schema.is_some());
    assert_eq!(schema.unwrap().name, "foxglove.GeoJSON");
}

/// Catches: broken `crate::encode::Encode` re-export, broken `crate::messages::{Log, Timestamp}`
/// paths, broken submodule path `crate::messages::log::Level`, or broken `Encode::encode()` impl.
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

/// Catches: broken `Timestamp::new()`, `sec()`, or `nsec()` accessors on the well-known type.
#[test]
fn test_timestamp_creation() {
    use crate::messages::Timestamp;

    let ts = Timestamp::new(123, 456);
    assert_eq!(ts.sec(), 123);
    assert_eq!(ts.nsec(), 456);
}

/// Catches: broken `crate::schemas` re-export, type mismatch between `schemas` and `messages`
/// modules, or encoding divergence between the two paths.
///
/// Note: In Rust, we can't easily test that a deprecation warning is emitted at runtime.
/// The `#[deprecated]` attribute emits warnings at compile time.
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

/// Catches: `crate::schemas` failing to re-export common types, or re-exporting a different
/// type than `crate::messages` (e.g., due to a version mismatch after crate extraction).
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

/// Catches: broken glob import from `crate::schemas::*`, which is the primary import pattern
/// used by downstream code.
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

/// Catches: broken `crate::protobuf::ProtobufField` re-export, or incorrect metadata
/// (field type, wire type, type name, file descriptors) on code-generated message types.
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
    let fds = Log::file_descriptors();
    assert!(
        fds.iter().any(|fd| fd.name() == "foxglove/Log.proto"),
        "expected foxglove/Log.proto descriptor, got: {:?}",
        fds.iter().map(|f| f.name()).collect::<Vec<_>>()
    );
}

/// Catches: broken hand-written `ProtobufField` impl for the `Timestamp` well-known type,
/// which is implemented separately from the code-generated message types.
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

/// Catches: broken `ProtobufField::write()` or `encoded_len()` producing invalid or
/// inconsistent length-delimited protobuf output.
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

/// Catches: broken `serde_enum_mod!` macro output — enums must serialize as string names
/// (e.g., `"ERROR"`) not integers, and must roundtrip through JSON correctly.
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

/// Catches: broken `serde_bytes` module — `Bytes` fields must serialize as base64 in JSON
/// and roundtrip correctly.
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
