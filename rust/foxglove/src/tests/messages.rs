//! Tests for the `messages` module and backward-compatible `schemas` module alias.
//!
//! The messages module contains well-known Foxglove message types. The schemas module
//! is deprecated and re-exports from messages for backward compatibility.

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
