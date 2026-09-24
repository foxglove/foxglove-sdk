//! Conversion from `sensor_msgs/PointCloud2` to `foxglove.PointCloud`.
//!
//! ROS 1 and ROS 2 define `PointCloud2` and `PointField` identically apart from the header,
//! so the wire-specific decoders ([`super::ros1`], [`super::ros2`]) each produce a
//! [`PointCloud2`] and share the layout validation and conversion here.

use serde::{Deserialize, Serialize};

use crate::messages::{PackedElementField, PointCloud, Timestamp, packed_element_field};

/// An error that occurs while converting a `PointCloud2` to a `foxglove.PointCloud`.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PointCloud2Error {
    /// The cloud is big-endian, which is not supported.
    #[error("big-endian point clouds are not supported")]
    BigEndian,
    /// A field has an unknown `PointField` datatype code.
    #[error("field {name:?} has unknown datatype {datatype}")]
    UnknownDatatype {
        /// The field name.
        name: String,
        /// The unrecognized datatype code.
        datatype: u8,
    },
    /// A field has `count != 1`, which `foxglove.PointCloud` cannot represent.
    #[error("field {name:?} has unsupported count {count} (only count 1 is supported)")]
    UnsupportedFieldCount {
        /// The field name.
        name: String,
        /// The unsupported element count.
        count: u32,
    },
    /// The data length is inconsistent with the declared dimensions.
    #[error(
        "data length {len} is smaller than the {expected} bytes implied by the declared dimensions"
    )]
    TruncatedData {
        /// The actual data length.
        len: usize,
        /// The data length implied by the declared dimensions.
        expected: usize,
    },
    /// The declared row stride is smaller than one row of points.
    #[error("row_step {row_step} is smaller than width {width} x point_step {point_step}")]
    RowStepTooSmall {
        /// The declared row stride in bytes.
        row_step: u32,
        /// The declared number of points per row.
        width: u32,
        /// The declared point stride in bytes.
        point_step: u32,
    },
    /// The declared dimensions overflow when multiplied.
    #[error(
        "cloud dimensions overflow (width {width}, height {height}, point_step {point_step}, \
         row_step {row_step})"
    )]
    DimensionsOverflow {
        /// The declared number of points per row.
        width: u32,
        /// The declared number of rows.
        height: u32,
        /// The declared point stride in bytes.
        point_step: u32,
        /// The declared row stride in bytes.
        row_step: u32,
    },
    /// The cloud declares zero points but carries point data.
    #[error(
        "cloud declares zero points (width {width}, height {height}) but carries {len} bytes \
         of data"
    )]
    ZeroDimensions {
        /// The declared number of points per row.
        width: u32,
        /// The declared number of rows.
        height: u32,
        /// The actual data length.
        len: usize,
    },
}

/// A `sensor_msgs/PointField` message.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct PointField {
    pub(crate) name: String,
    pub(crate) offset: u32,
    pub(crate) datatype: u8,
    pub(crate) count: u32,
}

impl PointField {
    /// Maps the `sensor_msgs/PointField` datatype constant to a
    /// `foxglove.PackedElementField` numeric type.
    ///
    /// The two enumerations order signed and unsigned integers differently, so this must
    /// be an explicit mapping rather than a numeric cast.
    fn numeric_type(&self) -> Result<packed_element_field::NumericType, PointCloud2Error> {
        use packed_element_field::NumericType;
        Ok(match self.datatype {
            1 => NumericType::Int8,
            2 => NumericType::Uint8,
            3 => NumericType::Int16,
            4 => NumericType::Uint16,
            5 => NumericType::Int32,
            6 => NumericType::Uint32,
            7 => NumericType::Float32,
            8 => NumericType::Float64,
            datatype => {
                return Err(PointCloud2Error::UnknownDatatype {
                    name: self.name.clone(),
                    datatype,
                });
            }
        })
    }
}

/// A decoded `sensor_msgs/PointCloud2` message, with its header already validated.
///
/// The `is_dense` flag is omitted: it is advisory (`false` means the cloud may contain
/// invalid, typically NaN-padded, points) and publishers set it unreliably in both
/// directions, so the transcode layer instead drops non-finite points from every cloud
/// before encoding (see `drop_non_finite_points`).
#[derive(Debug, PartialEq)]
pub(crate) struct PointCloud2 {
    pub(crate) timestamp: Timestamp,
    pub(crate) frame_id: String,
    pub(crate) height: u32,
    pub(crate) width: u32,
    pub(crate) fields: Vec<PointField>,
    pub(crate) is_bigendian: bool,
    pub(crate) point_step: u32,
    pub(crate) row_step: u32,
    pub(crate) data: Vec<u8>,
}

impl TryFrom<PointCloud2> for PointCloud {
    type Error = PointCloud2Error;

    fn try_from(cloud: PointCloud2) -> Result<Self, Self::Error> {
        if cloud.is_bigendian {
            return Err(PointCloud2Error::BigEndian);
        }

        let mut fields = Vec::with_capacity(cloud.fields.len());
        for field in &cloud.fields {
            if field.count != 1 {
                return Err(PointCloud2Error::UnsupportedFieldCount {
                    name: field.name.clone(),
                    count: field.count,
                });
            }
            fields.push(PackedElementField {
                name: field.name.clone(),
                offset: field.offset,
                r#type: field.numeric_type()? as i32,
            });
        }

        // A cloud that declares zero points (width or height 0) while carrying data is
        // contradictory, and trimming to the declared count would discard the entire
        // payload: silent data loss, while the same topic looks fine locally (the app
        // counts points by data length, not the declared dimensions). Reject it so the
        // publisher's misdeclaration surfaces as a channel warning instead. Zero
        // dimensions with an empty payload remain a legitimate empty cloud.
        if (cloud.width == 0 || cloud.height == 0) && !cloud.data.is_empty() {
            return Err(PointCloud2Error::ZeroDimensions {
                width: cloud.width,
                height: cloud.height,
                len: cloud.data.len(),
            });
        }

        // Organized clouds may pad each row to `row_step` bytes; `foxglove.PointCloud` has
        // no row stride, so repack rows contiguously when padding is present. Every
        // dimension here is untrusted wire data, so the arithmetic is checked and the
        // layout validated before any read or allocation.
        let overflow = || PointCloud2Error::DimensionsOverflow {
            width: cloud.width,
            height: cloud.height,
            point_step: cloud.point_step,
            row_step: cloud.row_step,
        };
        let packed_row_len = (cloud.width as usize)
            .checked_mul(cloud.point_step as usize)
            .ok_or_else(overflow)?;
        let data = if cloud.height > 1 && (cloud.row_step as usize) != packed_row_len {
            let height = cloud.height as usize;
            let row_step = cloud.row_step as usize;
            if row_step < packed_row_len {
                return Err(PointCloud2Error::RowStepTooSmall {
                    row_step: cloud.row_step,
                    width: cloud.width,
                    point_step: cloud.point_step,
                });
            }
            let needed = height.checked_mul(row_step).ok_or_else(overflow)?;
            if cloud.data.len() < needed {
                return Err(PointCloud2Error::TruncatedData {
                    len: cloud.data.len(),
                    expected: needed,
                });
            }
            // Every row read below ends at most at (height - 1) * row_step +
            // packed_row_len <= height * row_step <= data.len(), and the allocation is
            // bounded by height * packed_row_len <= height * row_step <= data.len().
            let mut packed = Vec::with_capacity(height * packed_row_len);
            for row in 0..height {
                let start = row * row_step;
                packed.extend_from_slice(&cloud.data[start..start + packed_row_len]);
            }
            packed
        } else {
            // `row_step` is deliberately not consulted here: unorganized (height <= 1)
            // publishers commonly leave it 0 or otherwise meaningless, so the declared
            // width x height is the source of truth for the point count instead. Short
            // data is an error, and any trailing bytes (row padding on a single-row
            // cloud, or excess payload) are trimmed so they cannot be misread as
            // phantom points downstream, where the count is data length / stride.
            let expected = packed_row_len
                .checked_mul(cloud.height as usize)
                .ok_or_else(overflow)?;
            if cloud.data.len() < expected {
                return Err(PointCloud2Error::TruncatedData {
                    len: cloud.data.len(),
                    expected,
                });
            }
            let mut data = cloud.data;
            data.truncate(expected);
            data
        };

        Ok(PointCloud {
            timestamp: Some(cloud.timestamp),
            frame_id: cloud.frame_id,
            // `PointCloud2` has no pose; the identity pose positions the cloud at the
            // origin of `frame_id`, matching ROS semantics.
            pose: None,
            point_stride: cloud.point_step,
            fields,
            data: data.into(),
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use packed_element_field::NumericType;

    pub(crate) fn xyz_fields() -> Vec<PointField> {
        [("x", 0), ("y", 4), ("z", 8)]
            .into_iter()
            .map(|(name, offset)| PointField {
                name: name.into(),
                offset,
                datatype: 7, // FLOAT32
                count: 1,
            })
            .collect()
    }

    pub(crate) fn cloud_data(points: &[[f32; 3]]) -> Vec<u8> {
        let mut data = Vec::with_capacity(points.len() * 12);
        for point in points {
            for c in point {
                data.extend_from_slice(&c.to_le_bytes());
            }
        }
        data
    }

    /// A float32 xyz `PointCloud2` with a single row of points.
    pub(crate) fn make_cloud(points: &[[f32; 3]]) -> PointCloud2 {
        PointCloud2 {
            timestamp: Timestamp::new(12, 34),
            frame_id: "lidar".into(),
            height: 1,
            width: points.len() as u32,
            fields: xyz_fields(),
            is_bigendian: false,
            point_step: 12,
            row_step: 12 * points.len() as u32,
            data: cloud_data(points),
        }
    }

    #[test]
    fn test_converts_point_cloud2() {
        let points = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]];
        let cloud = PointCloud::try_from(make_cloud(&points)).unwrap();

        assert_eq!(cloud.timestamp, Some(Timestamp::new(12, 34)));
        assert_eq!(cloud.frame_id, "lidar");
        assert_eq!(cloud.pose, None);
        assert_eq!(cloud.point_stride, 12);
        assert_eq!(cloud.fields.len(), 3);
        assert_eq!(cloud.fields[0].name, "x");
        assert_eq!(cloud.fields[0].offset, 0);
        assert_eq!(cloud.fields[0].r#type, NumericType::Float32 as i32);
        assert_eq!(cloud.data, cloud_data(&points));
    }

    #[test]
    fn test_maps_integer_datatypes_explicitly() {
        // PointField and NumericType order signed/unsigned differently.
        let cases = [
            (1, NumericType::Int8),
            (2, NumericType::Uint8),
            (3, NumericType::Int16),
            (4, NumericType::Uint16),
            (5, NumericType::Int32),
            (6, NumericType::Uint32),
            (7, NumericType::Float32),
            (8, NumericType::Float64),
        ];
        for (datatype, expected) in cases {
            let field = PointField {
                name: "f".into(),
                offset: 0,
                datatype,
                count: 1,
            };
            assert_eq!(field.numeric_type().unwrap(), expected);
        }
    }

    #[test]
    fn test_rejects_unknown_datatype() {
        let mut cloud = make_cloud(&[[0.0; 3]]);
        cloud.fields[0].datatype = 9;
        assert!(matches!(
            PointCloud::try_from(cloud),
            Err(PointCloud2Error::UnknownDatatype { datatype: 9, .. })
        ));
    }

    #[test]
    fn test_rejects_multi_element_fields() {
        let mut cloud = make_cloud(&[[0.0; 3]]);
        cloud.fields[0].count = 4;
        assert!(matches!(
            PointCloud::try_from(cloud),
            Err(PointCloud2Error::UnsupportedFieldCount { count: 4, .. })
        ));
    }

    #[test]
    fn test_rejects_big_endian() {
        let mut cloud = make_cloud(&[[0.0; 3]]);
        cloud.is_bigendian = true;
        assert!(matches!(
            PointCloud::try_from(cloud),
            Err(PointCloud2Error::BigEndian)
        ));
    }

    #[test]
    fn test_repacks_padded_rows() {
        // A 2x2 organized cloud with 8 bytes of padding per row.
        let points = [[1.0f32, 2.0, 3.0], [4.0, 5.0, 6.0]];
        let row: Vec<u8> = cloud_data(&points);
        let padded_row_step = 12 * 2 + 8;
        let mut data = Vec::new();
        for _ in 0..2 {
            data.extend_from_slice(&row);
            data.extend_from_slice(&[0u8; 8]);
        }

        let mut cloud = make_cloud(&points);
        cloud.height = 2;
        cloud.width = 2;
        cloud.row_step = padded_row_step;
        cloud.data = data;

        let converted = PointCloud::try_from(cloud).unwrap();
        let mut expected = row.clone();
        expected.extend_from_slice(&row);
        assert_eq!(converted.data, expected);
    }

    #[test]
    fn test_strips_padding_from_single_row_cloud() {
        // A height == 1 cloud whose single row is padded to row_step. The padding must
        // not survive conversion: foxglove.PointCloud has no width, so trailing bytes
        // would be misread as phantom points (data length / stride).
        let points = [[1.0f32, 2.0, 3.0], [4.0, 5.0, 6.0]];
        let mut data = cloud_data(&points);
        data.extend_from_slice(&[0u8; 8]);

        let mut cloud = make_cloud(&points);
        cloud.row_step = 12 * 2 + 8;
        cloud.data = data;

        let converted = PointCloud::try_from(cloud).unwrap();
        assert_eq!(converted.data, cloud_data(&points));
    }

    #[test]
    fn test_accepts_zero_row_step_on_single_row_cloud() {
        // Unorganized publishers commonly leave row_step 0; the declared width is the
        // source of truth, so conversion must not consult row_step for height == 1.
        let points = [[1.0f32, 2.0, 3.0], [4.0, 5.0, 6.0]];
        let mut cloud = make_cloud(&points);
        cloud.row_step = 0;

        let converted = PointCloud::try_from(cloud).unwrap();
        assert_eq!(converted.data, cloud_data(&points));
    }

    #[test]
    fn test_trims_excess_data_to_declared_dimensions() {
        // Data carrying more whole points than width x height declares is trimmed to
        // the declared dimensions rather than delivering phantom points.
        let points = [[1.0f32, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]];
        let mut cloud = make_cloud(&points[..2]);
        cloud.data = cloud_data(&points);

        let converted = PointCloud::try_from(cloud).unwrap();
        assert_eq!(converted.data, cloud_data(&points[..2]));
    }

    #[test]
    fn test_rejects_zero_dimensions_with_data() {
        // A cloud declaring zero points while carrying a payload would otherwise be
        // trimmed (or repacked) to nothing and delivered as an empty cloud — silent data
        // loss over remote access, while the same topic renders fine locally (the app
        // counts points by data length). All three shapes reach different branches, so
        // pin each: zero height (trim branch), zero width (trim branch), and zero width
        // with height > 1 (repack branch).
        let points = [[1.0f32, 2.0, 3.0], [4.0, 5.0, 6.0]];
        for (width, height) in [(2, 0), (0, 1), (0, 2)] {
            let mut cloud = make_cloud(&points);
            cloud.width = width;
            cloud.height = height;
            assert!(
                matches!(
                    PointCloud::try_from(cloud),
                    Err(PointCloud2Error::ZeroDimensions { len: 24, .. })
                ),
                "width {width}, height {height}"
            );
        }
    }

    #[test]
    fn test_accepts_zero_dimensions_with_empty_data() {
        // "Nothing detected this frame": zero declared points with an empty payload is a
        // legitimate empty cloud and must round-trip rather than error.
        let mut cloud = make_cloud(&[]);
        cloud.width = 0;
        cloud.height = 0;
        let converted = PointCloud::try_from(cloud).unwrap();
        assert!(converted.data.is_empty());
    }

    #[test]
    fn test_rejects_data_shorter_than_declared_dimensions() {
        let points = [[1.0f32, 2.0, 3.0], [4.0, 5.0, 6.0]];
        let mut cloud = make_cloud(&points);
        cloud.data = cloud_data(&points[..1]);

        assert!(matches!(
            PointCloud::try_from(cloud),
            Err(PointCloud2Error::TruncatedData {
                len: 12,
                expected: 24,
            })
        ));
    }

    #[test]
    fn test_rejects_row_step_smaller_than_row() {
        // Pre-validation, this layout read past the end of the buffer: 48 bytes of data,
        // rows read at start..start + 24 with start advancing by only 14.
        let mut cloud = make_cloud(&[[0.0; 3], [0.0; 3], [0.0; 3], [0.0; 3]]);
        cloud.height = 3;
        cloud.width = 2;
        cloud.row_step = 14;
        assert!(matches!(
            PointCloud::try_from(cloud),
            Err(PointCloud2Error::RowStepTooSmall {
                row_step: 14,
                width: 2,
                point_step: 12,
            })
        ));
    }

    #[test]
    fn test_rejects_absurd_dimensions_without_allocating() {
        // Hostile declared dimensions with a tiny payload must be rejected up front,
        // never used to size an allocation.
        let mut cloud = make_cloud(&[[0.0; 3]]);
        cloud.height = 2;
        cloud.width = u32::MAX;
        cloud.point_step = u32::MAX;
        cloud.row_step = u32::MAX;
        assert!(matches!(
            PointCloud::try_from(cloud),
            Err(PointCloud2Error::RowStepTooSmall { .. })
        ));
    }

    #[test]
    fn test_rejects_truncated_padded_data() {
        let mut cloud = make_cloud(&[[0.0; 3], [0.0; 3]]);
        cloud.height = 2;
        cloud.width = 2;
        cloud.row_step = 100;
        assert!(matches!(
            PointCloud::try_from(cloud),
            Err(PointCloud2Error::TruncatedData { .. })
        ));
    }
}
