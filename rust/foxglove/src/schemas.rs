//! Types implementing well-known Foxglove schemas
//!
//! Using these types when possible will allow for richer visualizations
//! and a better experience in the Foxglove App.
//!
//! They're encoded as compact, binary protobuf messages,
//! and can be conveniently used with the [`Channel`](crate::Channel) API.
//!
//! # Serde support
//!
//! The `serde` feature enables [`Serialize`](serde::Serialize) and
//! [`Deserialize`](serde::Deserialize) for all schema types. This is intended for debugging,
//! logging, and integration with tools that consume JSON or other serde-compatible formats.
//!
//! **This is not recommended as a wire protocol.** The current serialization has some quirks
//! that may change in future versions:
//!
//! - Enums are serialized as integers (their protobuf field values)
//! - Binary data is serialized as an array of integers in human-readable formats like JSON
//!
//! For efficient serialization, use the native protobuf encoding via the [`Encode`](crate::Encode)
//! trait.

pub(crate) mod descriptors;
#[allow(missing_docs)]
#[rustfmt::skip]
mod foxglove;
#[rustfmt::skip]
mod impls;

pub use self::foxglove::*;
pub use crate::schemas_wkt::{Duration, Timestamp};

#[cfg(test)]
#[cfg(feature = "serde")]
mod tests {
    use bytes::Bytes;

    use super::{
        packed_element_field::NumericType, Grid, PackedElementField, Pose, Quaternion, Timestamp,
        Vector2, Vector3,
    };

    fn sample_grid() -> Grid {
        // A message that has both binary and enum fields.
        Grid {
            timestamp: Some(Timestamp::new(1234567890, 123456789)),
            frame_id: "map".to_string(),
            pose: Some(Pose {
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
            }),
            column_count: 10,
            cell_size: Some(Vector2 { x: 0.1, y: 0.1 }),
            row_stride: 40,
            cell_stride: 4,
            fields: vec![PackedElementField {
                name: "elevation".to_string(),
                offset: 0,
                r#type: NumericType::Float32 as i32,
            }],
            data: Bytes::from_static(&[0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x40]),
        }
    }

    #[test]
    fn test_grid_json_snapshot() {
        let grid = sample_grid();
        let json = serde_json::to_value(&grid).expect("failed to serialize");
        insta::assert_json_snapshot!(json);
    }

    #[test]
    fn test_grid_json_roundtrip() {
        let grid = sample_grid();
        let json = serde_json::to_string(&grid).expect("failed to serialize");
        let parsed: Grid = serde_json::from_str(&json).expect("failed to deserialize");
        assert_eq!(grid, parsed);
    }
}
