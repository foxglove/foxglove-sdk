//! Types implementing well-known Foxglove message types.
//!
//! Using these types when possible will allow for richer visualizations and a better experience
//! in the Foxglove App. They are encoded as compact, binary protobuf messages and can be
//! conveniently used with the [`Channel`](crate::Channel) API.
//!
//! # Serde support
//!
//! The `serde` feature enables [`Serialize`](serde::Serialize) and
//! [`Deserialize`](serde::Deserialize) for all message types. This is intended for debugging,
//! logging, and integration with tools that consume JSON or other serde-compatible formats.
//!
//! For human-readable formats (e.g., JSON), enums are serialized as string names, and binary data
//! are serialized as base64. For binary formats, enums are serialized as i32 values.
//!
//! Note that [CDR](https://docs.rs/cdr) is not compatible with these message types, because it does
//! not support optional fields.

pub(crate) mod descriptors;
#[allow(missing_docs)]
#[rustfmt::skip]
mod foxglove;
#[rustfmt::skip]
mod impls;

pub use self::foxglove::*;
pub use crate::messages_wkt::{Duration, Timestamp};

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

    impl Visitor<'_> for BytesVisitor {
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

#[cfg(test)]
#[cfg(feature = "serde")]
mod tests {
    use bytes::Bytes;

    use super::{
        Grid, PackedElementField, Pose, Quaternion, Timestamp, Vector2, Vector3,
        packed_element_field::NumericType,
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
}
