//! Types implementing well-known Foxglove schemas
//!
//! Using these types when possible will allow for richer visualizations and a better experience
//! in the Foxglove App. They are encoded as compact, binary protobuf messages and can be
//! conveniently used with the [`Channel`](crate::Channel) API.
//!
//! # Serde support
//!
//! The `serde` feature enables [`Serialize`](serde::Serialize) and
//! [`Deserialize`](serde::Deserialize) for all schema types. This is intended for debugging,
//! logging, and integration with tools that consume JSON or other serde-compatible formats.
//!
//! For human-readable formats (e.g., JSON), enums are serialized as string names, and binary data
//! are serialized as base64. For binary formats, enums are serialized as i32 values.
//!
//! Note that [CDR](https://docs.rs/cdr) is not compatible with these schemas, because it does
//! not support optional fields.

pub(crate) mod descriptors;
#[allow(missing_docs)]
#[rustfmt::skip]
mod foxglove;
#[rustfmt::skip]
mod impls;

pub use self::foxglove::*;
pub use crate::schemas_wkt::{Duration, Timestamp};

/// Custom serde serialization for `bytes::Bytes`.
///
/// Uses base64 encoding for human-readable formats (JSON) and raw bytes for binary formats.
#[cfg(feature = "serde")]
pub(crate) mod serde_bytes {
    use base64::Engine;
    use bytes::Bytes;
    use serde::de::{Error as _, Visitor};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Bytes, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if s.is_human_readable() {
            let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
            s.serialize_str(&b64)
        } else {
            s.serialize_bytes(bytes)
        }
    }

    pub fn deserialize<'de, D>(d: D) -> Result<Bytes, D::Error>
    where
        D: Deserializer<'de>,
    {
        if d.is_human_readable() {
            let s = String::deserialize(d)?;
            let data = base64::engine::general_purpose::STANDARD
                .decode(s)
                .map_err(D::Error::custom)?;
            Ok(Bytes::from(data))
        } else {
            d.deserialize_byte_buf(BytesVisitor)
        }
    }

    struct BytesVisitor;

    impl<'de> Visitor<'de> for BytesVisitor {
        type Value = Bytes;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a byte array")
        }

        fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Bytes::copy_from_slice(v))
        }

        fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Bytes::from(v))
        }
    }
}

/// Generates a serde module for a protobuf enum field.
///
/// Uses string names for human-readable formats (JSON) and i32 for binary formats.
#[cfg(feature = "serde")]
macro_rules! serde_enum_mod {
    ($mod_name:ident, $enum_path:ty) => {
        pub mod $mod_name {
            use super::*;

            pub fn serialize<S>(v: &i32, s: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                if s.is_human_readable() {
                    let e = <$enum_path>::try_from(*v)
                        .map_err(|_| serde::ser::Error::custom("invalid enum value"))?;
                    s.serialize_str(e.as_str_name())
                } else {
                    s.serialize_i32(*v)
                }
            }

            pub fn deserialize<'de, D>(d: D) -> Result<i32, D::Error>
            where
                D: Deserializer<'de>,
            {
                if d.is_human_readable() {
                    let s = String::deserialize(d)?;
                    let e = <$enum_path>::from_str_name(&s)
                        .ok_or_else(|| D::Error::custom("invalid enum string"))?;
                    Ok(e as i32)
                } else {
                    i32::deserialize(d)
                }
            }
        }
    };
}

#[cfg(feature = "serde")]
pub(crate) use serde_enum_mod;

/// Generates a serde module for an optional protobuf enum field.
///
/// Uses string names for human-readable formats (JSON) and i32 for binary formats.
#[cfg(feature = "serde")]
macro_rules! serde_enum_mod_optional {
    ($mod_name:ident, $enum_path:ty) => {
        pub mod $mod_name {
            use super::*;

            pub fn serialize<S>(v: &Option<i32>, s: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                match v {
                    Some(v) => {
                        if s.is_human_readable() {
                            let e = <$enum_path>::try_from(*v)
                                .map_err(|_| serde::ser::Error::custom("invalid enum value"))?;
                            s.serialize_some(e.as_str_name())
                        } else {
                            s.serialize_some(v)
                        }
                    }
                    None => s.serialize_none(),
                }
            }

            pub fn deserialize<'de, D>(d: D) -> Result<Option<i32>, D::Error>
            where
                D: Deserializer<'de>,
            {
                if d.is_human_readable() {
                    let opt: Option<String> = Option::deserialize(d)?;
                    match opt {
                        Some(s) => {
                            let e = <$enum_path>::from_str_name(&s)
                                .ok_or_else(|| D::Error::custom("invalid enum string"))?;
                            Ok(Some(e as i32))
                        }
                        None => Ok(None),
                    }
                } else {
                    Option::<i32>::deserialize(d)
                }
            }
        }
    };
}

#[cfg(feature = "serde")]
pub(crate) use serde_enum_mod_optional;

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

    #[test]
    fn test_grid_cbor_snapshot() {
        let grid = sample_grid();
        let bytes = serde_cbor::to_vec(&grid).expect("failed to serialize");
        insta::assert_snapshot!(format!("{bytes:#04x?}"));
    }

    #[test]
    fn test_grid_cbor_roundtrip() {
        let grid = sample_grid();
        let bytes = serde_cbor::to_vec(&grid).expect("failed to serialize");
        let parsed: Grid = serde_cbor::from_slice(&bytes).expect("failed to deserialize");
        assert_eq!(grid, parsed);
    }

    #[test]
    fn test_location_fix_json_roundtrip_with_point_style() {
        use super::{location_fix::PointStyle, LocationFix};

        let fix = LocationFix {
            latitude: 37.7749,
            longitude: -122.4194,
            altitude: 10.0,
            point_style: Some(PointStyle::Pin as i32),
            ..Default::default()
        };
        let json = serde_json::to_string(&fix).expect("failed to serialize");
        assert!(json.contains("\"point_style\":\"PIN\""));
        let parsed: LocationFix = serde_json::from_str(&json).expect("failed to deserialize");
        assert_eq!(fix, parsed);
    }

    #[test]
    fn test_location_fix_json_roundtrip_with_none_point_style() {
        use super::LocationFix;

        let fix = LocationFix {
            latitude: 37.7749,
            longitude: -122.4194,
            point_style: None,
            ..Default::default()
        };
        let json = serde_json::to_string(&fix).expect("failed to serialize");
        assert!(json.contains("\"point_style\":null"));
        let parsed: LocationFix = serde_json::from_str(&json).expect("failed to deserialize");
        assert_eq!(fix, parsed);
    }

    #[test]
    fn test_location_fix_json_invalid_point_style() {
        use super::LocationFix;

        let json = r#"{"point_style":"INVALID_VALUE","latitude":0.0,"longitude":0.0,"altitude":0.0,"position_covariance":[],"position_covariance_type":"UNKNOWN"}"#;
        let result: Result<LocationFix, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_location_fix_json_missing_point_style() {
        use super::LocationFix;

        // JSON without point_style field at all — must deserialize with point_style: None
        let json = r#"{"latitude":37.7749,"longitude":-122.4194,"altitude":0.0,"frame_id":"","position_covariance":[],"position_covariance_type":"UNKNOWN"}"#;
        let parsed: LocationFix = serde_json::from_str(json).expect("failed to deserialize");
        assert_eq!(parsed.point_style, None);
        assert_eq!(parsed.latitude, 37.7749);
    }
}
