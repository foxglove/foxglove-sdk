use bytes::BytesMut;
use foxglove::Encode;
use foxglove::schemas::{Log, Point3, Pose, Quaternion, Timestamp, Vector3};
use prost::Message;
use prost_reflect::DescriptorPool;

/// Test nesting a simple foxglove schema type (Log).
#[derive(Encode)]
struct MessageWithLog {
    log: Log,
    value: u32,
}

/// Test nesting foxglove schema types that have their own nested types.
#[derive(Encode)]
struct MessageWithPose {
    pose: Pose,
    name: String,
}

/// Test Vec of foxglove schema types.
#[derive(Encode)]
struct MessageWithPoints {
    points: Vec<Point3>,
}

/// Test Option of foxglove schema type.
#[derive(Encode)]
struct MessageWithOptionalLog {
    log: Option<Log>,
}

#[test]
fn test_nested_foxglove_log() {
    let test_struct = MessageWithLog {
        log: Log {
            timestamp: Some(Timestamp::new(1234567890, 123456789)),
            level: foxglove::schemas::log::Level::Info as i32,
            message: "Hello from nested log".to_string(),
            name: "test".to_string(),
            file: "test.rs".to_string(),
            line: 42,
        },
        value: 123,
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("encode failed");

    // Verify schema is valid and parseable
    let schema = MessageWithLog::get_schema().expect("schema");
    let fds = prost_types::FileDescriptorSet::decode(schema.data.as_ref()).expect("decode schema");

    // Should include foxglove.Log file descriptor
    assert!(
        fds.file.iter().any(|f| f.name() == "foxglove/Log.proto"),
        "Log.proto should be included"
    );

    // Schema should be parseable
    let pool = DescriptorPool::from_file_descriptor_set(fds)
        .expect("schema should be valid and parseable");

    assert!(
        pool.get_message_by_name(&schema.name).is_some(),
        "message should be in the pool"
    );
}

#[test]
fn test_nested_foxglove_pose() {
    let test_struct = MessageWithPose {
        pose: Pose {
            position: Some(Vector3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            }),
            orientation: Some(Quaternion {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0,
            }),
        },
        name: "test pose".to_string(),
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("encode failed");

    let schema = MessageWithPose::get_schema().expect("schema");
    let fds = prost_types::FileDescriptorSet::decode(schema.data.as_ref()).expect("decode schema");

    // Should include Pose and its dependencies (Vector3, Quaternion)
    assert!(
        fds.file.iter().any(|f| f.name() == "foxglove/Pose.proto"),
        "Pose.proto should be included"
    );

    let pool = DescriptorPool::from_file_descriptor_set(fds)
        .expect("schema should be valid and parseable");

    assert!(
        pool.get_message_by_name(&schema.name).is_some(),
        "message should be in the pool"
    );
}

#[test]
fn test_vec_of_foxglove_points() {
    let test_struct = MessageWithPoints {
        points: vec![
            Point3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            Point3 {
                x: 4.0,
                y: 5.0,
                z: 6.0,
            },
        ],
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("encode failed");

    let schema = MessageWithPoints::get_schema().expect("schema");
    let fds = prost_types::FileDescriptorSet::decode(schema.data.as_ref()).expect("decode schema");

    // Should include Point3
    assert!(
        fds.file.iter().any(|f| f.name() == "foxglove/Point3.proto"),
        "Point3.proto should be included"
    );

    let pool = DescriptorPool::from_file_descriptor_set(fds)
        .expect("schema should be valid and parseable");

    assert!(
        pool.get_message_by_name(&schema.name).is_some(),
        "message should be in the pool"
    );
}

#[test]
fn test_optional_foxglove_log() {
    // Test with Some
    let test_struct = MessageWithOptionalLog {
        log: Some(Log {
            timestamp: None,
            level: foxglove::schemas::log::Level::Warning as i32,
            message: "Warning!".to_string(),
            name: "".to_string(),
            file: "".to_string(),
            line: 0,
        }),
    };

    let mut buf = BytesMut::new();
    test_struct.encode(&mut buf).expect("encode failed");

    // Test with None
    let test_struct_none = MessageWithOptionalLog { log: None };
    let mut buf_none = BytesMut::new();
    test_struct_none
        .encode(&mut buf_none)
        .expect("encode failed");

    // None should encode to empty (or minimal) buffer
    assert!(
        buf_none.len() < buf.len(),
        "None should encode to less data"
    );

    let schema = MessageWithOptionalLog::get_schema().expect("schema");
    let fds = prost_types::FileDescriptorSet::decode(schema.data.as_ref()).expect("decode schema");

    let pool = DescriptorPool::from_file_descriptor_set(fds)
        .expect("schema should be valid and parseable");

    assert!(
        pool.get_message_by_name(&schema.name).is_some(),
        "message should be in the pool"
    );
}
