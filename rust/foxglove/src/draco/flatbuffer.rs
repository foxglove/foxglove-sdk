//! FlatBuffer `foxglove.PointCloud` decoder for point-cloud compression.
//!
//! Follows the hand-rolled vtable accessor pattern established by `img2yuv/flatbuffer.rs`:
//! verification markers run the FlatBuffer verifier over every variable-length field, so
//! the unsafe reads below are justified by the verifier, and inline structs (which the
//! verifier does not cover) are read with explicit bounds checks.

use flatbuffers::{
    Follow, ForwardsUOffset, InvalidFlatbuffer, Table, VOffsetT, Vector, Verifiable, Verifier,
};

use crate::messages::{PackedElementField, PointCloud, Pose, Quaternion, Timestamp, Vector3};

/// An error that occurs while decoding a FlatBuffer point cloud message.
#[derive(Debug, thiserror::Error)]
pub(crate) enum FlatbufferPointCloudError {
    /// The buffer is not a valid FlatBuffer.
    #[error("invalid flatbuffer: {0}")]
    Invalid(#[from] InvalidFlatbuffer),
    /// The timestamp cannot be represented.
    #[error("timestamp out of range")]
    InvalidTimestamp,
}

// VTable byte offsets for the fields, derived from each table's field ids in the `.fbs`
// schema. Field id `n` lives at vtable byte offset `4 + 2 * n`.
mod point_cloud {
    use flatbuffers::VOffsetT;
    pub(super) const VT_TIMESTAMP: VOffsetT = 4;
    pub(super) const VT_FRAME_ID: VOffsetT = 6;
    pub(super) const VT_POSE: VOffsetT = 8;
    pub(super) const VT_POINT_STRIDE: VOffsetT = 10;
    pub(super) const VT_FIELDS: VOffsetT = 12;
    pub(super) const VT_DATA: VOffsetT = 14;
}
mod pose {
    use flatbuffers::VOffsetT;
    pub(super) const VT_POSITION: VOffsetT = 4;
    pub(super) const VT_ORIENTATION: VOffsetT = 6;
}
mod vector3 {
    use flatbuffers::VOffsetT;
    pub(super) const VT_X: VOffsetT = 4;
    pub(super) const VT_Y: VOffsetT = 6;
    pub(super) const VT_Z: VOffsetT = 8;
}
mod quaternion {
    use flatbuffers::VOffsetT;
    pub(super) const VT_X: VOffsetT = 4;
    pub(super) const VT_Y: VOffsetT = 6;
    pub(super) const VT_Z: VOffsetT = 8;
    pub(super) const VT_W: VOffsetT = 10;
}
mod packed_element_field {
    use flatbuffers::VOffsetT;
    pub(super) const VT_NAME: VOffsetT = 4;
    pub(super) const VT_OFFSET: VOffsetT = 6;
    pub(super) const VT_TYPE: VOffsetT = 8;
}

/// Declares a verification marker type: a [`Follow`] impl yielding a raw [`Table`], plus a
/// [`Verifiable`] impl that type-checks the table's variable-length fields so the accessors
/// can read them safely.
macro_rules! table_marker {
    ($name:ident, |$v:ident, $pos:ident| $verify:expr) => {
        struct $name;
        impl<'a> Follow<'a> for $name {
            type Inner = Table<'a>;
            unsafe fn follow(buf: &'a [u8], loc: usize) -> Self::Inner {
                unsafe { Table::follow(buf, loc) }
            }
        }
        impl Verifiable for $name {
            fn run_verifier($v: &mut Verifier, $pos: usize) -> Result<(), InvalidFlatbuffer> {
                $verify
            }
        }
    };
}

table_marker!(Vector3Table, |v, pos| {
    v.visit_table(pos)?
        .visit_field::<f64>("x", vector3::VT_X, false)?
        .visit_field::<f64>("y", vector3::VT_Y, false)?
        .visit_field::<f64>("z", vector3::VT_Z, false)?
        .finish();
    Ok(())
});

table_marker!(QuaternionTable, |v, pos| {
    v.visit_table(pos)?
        .visit_field::<f64>("x", quaternion::VT_X, false)?
        .visit_field::<f64>("y", quaternion::VT_Y, false)?
        .visit_field::<f64>("z", quaternion::VT_Z, false)?
        .visit_field::<f64>("w", quaternion::VT_W, false)?
        .finish();
    Ok(())
});

table_marker!(PoseTable, |v, pos| {
    v.visit_table(pos)?
        .visit_field::<ForwardsUOffset<Vector3Table>>("position", pose::VT_POSITION, false)?
        .visit_field::<ForwardsUOffset<QuaternionTable>>(
            "orientation",
            pose::VT_ORIENTATION,
            false,
        )?
        .finish();
    Ok(())
});

table_marker!(PackedElementFieldTable, |v, pos| {
    v.visit_table(pos)?
        .visit_field::<ForwardsUOffset<&str>>("name", packed_element_field::VT_NAME, false)?
        .visit_field::<u32>("offset", packed_element_field::VT_OFFSET, false)?
        .visit_field::<u8>("type", packed_element_field::VT_TYPE, false)?
        .finish();
    Ok(())
});

table_marker!(PointCloudTable, |v, pos| {
    v.visit_table(pos)?
        .visit_field::<ForwardsUOffset<&str>>("frame_id", point_cloud::VT_FRAME_ID, false)?
        .visit_field::<ForwardsUOffset<PoseTable>>("pose", point_cloud::VT_POSE, false)?
        .visit_field::<u32>("point_stride", point_cloud::VT_POINT_STRIDE, false)?
        .visit_field::<ForwardsUOffset<Vector<ForwardsUOffset<PackedElementFieldTable>>>>(
            "fields",
            point_cloud::VT_FIELDS,
            false,
        )?
        .visit_field::<ForwardsUOffset<Vector<u8>>>("data", point_cloud::VT_DATA, false)?
        .finish();
    Ok(())
});

/// Reads a little-endian `u32` at `loc`, or `None` if out of bounds.
fn read_u32_le(buf: &[u8], loc: usize) -> Option<u32> {
    buf.get(loc..loc + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

/// Reads the inline `foxglove.Time` struct at the given vtable slot.
///
/// Returns `None` if the field is absent. FlatBuffer verification covers the table and its
/// variable-length fields but not inline structs, so this read is bounds-checked and
/// rejects out-of-range timestamps.
fn read_timestamp(
    table: &Table,
    slot: VOffsetT,
) -> Result<Option<Timestamp>, FlatbufferPointCloudError> {
    let offset = table.vtable().get(slot);
    if offset == 0 {
        return Ok(None);
    }
    let loc = table.loc() + offset as usize;
    let buf = table.buf();
    let sec = read_u32_le(buf, loc).ok_or(FlatbufferPointCloudError::InvalidTimestamp)?;
    let nsec = read_u32_le(buf, loc + 4).ok_or(FlatbufferPointCloudError::InvalidTimestamp)?;
    Timestamp::new_checked(sec, nsec)
        .map(Some)
        .ok_or(FlatbufferPointCloudError::InvalidTimestamp)
}

/// Reads the `foxglove.Pose` table at the given vtable slot, if present.
///
/// # Safety
/// The caller must have verified `table` with a marker that checks `slot` holds a
/// `PoseTable`.
unsafe fn read_pose(table: &Table, slot: VOffsetT) -> Option<Pose> {
    // Safety: verified as PoseTable / Vector3Table / QuaternionTable by the caller's
    // verification marker; scalar `get` calls read verified table fields.
    unsafe {
        let pose = table.get::<ForwardsUOffset<PoseTable>>(slot, None)?;
        let position = pose
            .get::<ForwardsUOffset<Vector3Table>>(pose::VT_POSITION, None)
            .map(|t| Vector3 {
                x: t.get::<f64>(vector3::VT_X, Some(0.0)).unwrap_or(0.0),
                y: t.get::<f64>(vector3::VT_Y, Some(0.0)).unwrap_or(0.0),
                z: t.get::<f64>(vector3::VT_Z, Some(0.0)).unwrap_or(0.0),
            });
        let orientation = pose
            .get::<ForwardsUOffset<QuaternionTable>>(pose::VT_ORIENTATION, None)
            .map(|t| Quaternion {
                x: t.get::<f64>(quaternion::VT_X, Some(0.0)).unwrap_or(0.0),
                y: t.get::<f64>(quaternion::VT_Y, Some(0.0)).unwrap_or(0.0),
                z: t.get::<f64>(quaternion::VT_Z, Some(0.0)).unwrap_or(0.0),
                // The schema declares `w = 1.0` as the field default, so an absent `w`
                // (elided by builders when equal to the default) reads back as 1.0.
                w: t.get::<f64>(quaternion::VT_W, Some(1.0)).unwrap_or(1.0),
            });
        Some(Pose {
            position,
            orientation,
        })
    }
}

/// Decodes a FlatBuffer-encoded `foxglove.PointCloud`.
pub(crate) fn decode_point_cloud(data: &[u8]) -> Result<PointCloud, FlatbufferPointCloudError> {
    let table = flatbuffers::root::<PointCloudTable>(data)?;
    // Safety: the fields were verified to have these types by `PointCloudTable`.
    let frame_id = unsafe {
        table
            .get::<ForwardsUOffset<&str>>(point_cloud::VT_FRAME_ID, Some(""))
            .unwrap_or("")
    };
    let point_stride = unsafe {
        table
            .get::<u32>(point_cloud::VT_POINT_STRIDE, Some(0))
            .unwrap_or(0)
    };
    let bytes = unsafe {
        table
            .get::<ForwardsUOffset<Vector<u8>>>(point_cloud::VT_DATA, None)
            .map(|v| v.bytes())
            .unwrap_or(&[])
    };
    let fields = unsafe {
        table
            .get::<ForwardsUOffset<Vector<ForwardsUOffset<PackedElementFieldTable>>>>(
                point_cloud::VT_FIELDS,
                None,
            )
            .map(|fields| {
                fields
                    .iter()
                    .map(|field| PackedElementField {
                        name: field
                            .get::<ForwardsUOffset<&str>>(packed_element_field::VT_NAME, Some(""))
                            .unwrap_or("")
                            .to_string(),
                        offset: field
                            .get::<u32>(packed_element_field::VT_OFFSET, Some(0))
                            .unwrap_or(0),
                        // FlatBuffer NumericType values match the protobuf enum exactly;
                        // out-of-range values are rejected downstream by the Draco encoder.
                        r#type: i32::from(
                            field
                                .get::<u8>(packed_element_field::VT_TYPE, Some(0))
                                .unwrap_or(0),
                        ),
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    // Safety: `pose` was verified as a PoseTable by `PointCloudTable`.
    let pose = unsafe { read_pose(&table, point_cloud::VT_POSE) };

    Ok(PointCloud {
        timestamp: read_timestamp(&table, point_cloud::VT_TIMESTAMP)?,
        frame_id: frame_id.to_string(),
        pose,
        point_stride,
        fields,
        data: bytes.to_vec().into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flatbuffers::{FlatBufferBuilder, Push, WIPOffset};

    /// Test-side encoder for the inline `foxglove.Time` struct.
    struct TimeStruct {
        sec: u32,
        nsec: u32,
    }
    impl Push for TimeStruct {
        type Output = TimeStruct;
        unsafe fn push(&self, dst: &mut [u8], _written: usize) {
            dst[0..4].copy_from_slice(&self.sec.to_le_bytes());
            dst[4..8].copy_from_slice(&self.nsec.to_le_bytes());
        }
        fn size() -> usize {
            8
        }
        fn alignment() -> flatbuffers::PushAlignment {
            flatbuffers::PushAlignment::new(4)
        }
    }

    fn build_vector3(fbb: &mut FlatBufferBuilder, x: f64, y: f64, z: f64) -> WIPOffset<()> {
        let start = fbb.start_table();
        fbb.push_slot::<f64>(vector3::VT_X, x, 0.0);
        fbb.push_slot::<f64>(vector3::VT_Y, y, 0.0);
        fbb.push_slot::<f64>(vector3::VT_Z, z, 0.0);
        WIPOffset::new(fbb.end_table(start).value())
    }

    fn build_quaternion(fbb: &mut FlatBufferBuilder, x: f64, w: f64) -> WIPOffset<()> {
        let start = fbb.start_table();
        fbb.push_slot::<f64>(quaternion::VT_X, x, 0.0);
        // y and z left at default; w elided when it equals the schema default of 1.0.
        fbb.push_slot::<f64>(quaternion::VT_W, w, 1.0);
        WIPOffset::new(fbb.end_table(start).value())
    }

    struct TestCloud {
        with_timestamp: bool,
        with_pose: bool,
        quaternion_w: f64,
    }

    impl Default for TestCloud {
        fn default() -> Self {
            Self {
                with_timestamp: true,
                with_pose: true,
                quaternion_w: 0.5,
            }
        }
    }

    /// Builds a FlatBuffer-encoded `foxglove.PointCloud` with one float32 xyz point.
    fn build_point_cloud(spec: &TestCloud) -> Vec<u8> {
        let mut fbb = FlatBufferBuilder::new();

        let mut data = Vec::new();
        for c in [1.0f32, 2.0, 3.0] {
            data.extend_from_slice(&c.to_le_bytes());
        }
        let data_vec = fbb.create_vector::<u8>(&data);

        let mut field_offsets = Vec::new();
        for (i, name) in ["x", "y", "z"].into_iter().enumerate() {
            let name = fbb.create_string(name);
            let start = fbb.start_table();
            fbb.push_slot_always(packed_element_field::VT_NAME, name);
            fbb.push_slot::<u32>(packed_element_field::VT_OFFSET, 4 * i as u32, 0);
            fbb.push_slot::<u8>(packed_element_field::VT_TYPE, 7, 0); // FLOAT32
            field_offsets.push(WIPOffset::<()>::new(fbb.end_table(start).value()));
        }
        let fields_vec = fbb.create_vector(&field_offsets);

        let pose = if spec.with_pose {
            let position = build_vector3(&mut fbb, 1.0, 2.0, 3.0);
            let orientation = build_quaternion(&mut fbb, 0.25, spec.quaternion_w);
            let start = fbb.start_table();
            fbb.push_slot_always(pose::VT_POSITION, position);
            fbb.push_slot_always(pose::VT_ORIENTATION, orientation);
            Some(WIPOffset::<()>::new(fbb.end_table(start).value()))
        } else {
            None
        };

        let frame_id = fbb.create_string("lidar");
        let start = fbb.start_table();
        if spec.with_timestamp {
            fbb.push_slot_always(point_cloud::VT_TIMESTAMP, TimeStruct { sec: 12, nsec: 34 });
        }
        fbb.push_slot_always(point_cloud::VT_FRAME_ID, frame_id);
        if let Some(pose) = pose {
            fbb.push_slot_always(point_cloud::VT_POSE, pose);
        }
        fbb.push_slot::<u32>(point_cloud::VT_POINT_STRIDE, 12, 0);
        fbb.push_slot_always(point_cloud::VT_FIELDS, fields_vec);
        fbb.push_slot_always(point_cloud::VT_DATA, data_vec);
        let root = fbb.end_table(start);
        fbb.finish_minimal(root);
        fbb.finished_data().to_vec()
    }

    #[test]
    fn test_decodes_full_point_cloud() {
        let buf = build_point_cloud(&TestCloud::default());
        let cloud = decode_point_cloud(&buf).unwrap();

        assert_eq!(cloud.timestamp, Some(Timestamp::new(12, 34)));
        assert_eq!(cloud.frame_id, "lidar");
        assert_eq!(cloud.point_stride, 12);
        assert_eq!(cloud.fields.len(), 3);
        assert_eq!(cloud.fields[0].name, "x");
        assert_eq!(cloud.fields[1].offset, 4);
        assert_eq!(cloud.fields[2].r#type, 7);
        assert_eq!(cloud.data.len(), 12);

        let pose = cloud.pose.expect("pose should be present");
        let position = pose.position.expect("position should be present");
        assert_eq!((position.x, position.y, position.z), (1.0, 2.0, 3.0));
        let orientation = pose.orientation.expect("orientation should be present");
        assert_eq!(orientation.x, 0.25);
        assert_eq!(orientation.w, 0.5);
    }

    #[test]
    fn test_absent_quaternion_w_reads_schema_default() {
        // A builder elides `w` when it equals the schema default of 1.0; the decoder must
        // read the default back rather than 0.
        let buf = build_point_cloud(&TestCloud {
            quaternion_w: 1.0,
            ..Default::default()
        });
        let cloud = decode_point_cloud(&buf).unwrap();
        let orientation = cloud.pose.unwrap().orientation.unwrap();
        assert_eq!(orientation.w, 1.0);
    }

    #[test]
    fn test_absent_optional_fields() {
        let buf = build_point_cloud(&TestCloud {
            with_timestamp: false,
            with_pose: false,
            ..Default::default()
        });
        let cloud = decode_point_cloud(&buf).unwrap();
        assert_eq!(cloud.timestamp, None);
        assert_eq!(cloud.pose, None);
        assert_eq!(cloud.frame_id, "lidar");
    }

    #[test]
    fn test_transcodes_to_compressed_point_cloud() {
        use crate::Decode;
        use crate::draco::transcode::{PointCloudInputSchema, transcode_point_cloud_message};

        let buf = build_point_cloud(&TestCloud::default());
        let compressed_bytes = transcode_point_cloud_message(
            &buf,
            PointCloudInputSchema::FoxgloveFlatbuffer,
            &crate::draco::CompressPointCloudOptions::default(),
        )
        .unwrap();
        let compressed =
            <crate::messages::CompressedPointCloud as Decode>::decode(compressed_bytes.as_ref())
                .unwrap();
        assert_eq!(compressed.format, "draco");
        assert_eq!(compressed.frame_id, "lidar");
        // The pose survives transcoding into the CompressedPointCloud wrapper.
        assert!(compressed.pose.is_some());
        assert!(!compressed.data.is_empty());
    }

    #[test]
    fn test_rejects_invalid_buffer() {
        assert!(matches!(
            decode_point_cloud(&[0x01, 0x02, 0x03]),
            Err(FlatbufferPointCloudError::Invalid(_))
        ));
        // A valid buffer with a trailing truncation is also rejected by verification.
        let mut buf = build_point_cloud(&TestCloud::default());
        buf.truncate(buf.len() / 2);
        assert!(decode_point_cloud(&buf).is_err());
    }

    // Golden FlatBuffer-encoded PointCloud messages, generated with `flatc -b` (v25.12.19)
    // from the canonical `schemas/flatbuffer/PointCloud.fbs`, independently of the
    // decoder's hand-written vtable offset constants, so the tests below catch any drift
    // in those constants. Contents: timestamp {sec: 12, nsec: 34}, frame_id "lidar",
    // pose {position (1, 2, 3), orientation (0.25, 0, 0, w)}, point_stride 12, float32
    // x/y/z fields at offsets 0/4/8, and one point (1.0, 2.0, 3.0).
    const GOLDEN_FULL: &[u8] = &[
        0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x20, 0x00, 0x04, 0x00, 0x0c,
        0x00, 0x10, 0x00, 0x14, 0x00, 0x18, 0x00, 0x1c, 0x00, 0x10, 0x00, 0x00, 0x00, 0x0c, 0x00,
        0x00, 0x00, 0x22, 0x00, 0x00, 0x00, 0xe8, 0x00, 0x00, 0x00, 0x8c, 0x00, 0x00, 0x00, 0x0c,
        0x00, 0x00, 0x00, 0x18, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x40, 0x40, 0x03, 0x00, 0x00,
        0x00, 0x4c, 0x00, 0x00, 0x00, 0x28, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xea, 0xff,
        0xff, 0xff, 0x00, 0x00, 0x00, 0x07, 0x08, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01,
        0x00, 0x00, 0x00, 0x7a, 0x00, 0x0a, 0x00, 0x10, 0x00, 0x08, 0x00, 0x0c, 0x00, 0x07, 0x00,
        0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x08, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00,
        0x00, 0x01, 0x00, 0x00, 0x00, 0x79, 0x00, 0x0a, 0x00, 0x0c, 0x00, 0x08, 0x00, 0x00, 0x00,
        0x07, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x04, 0x00, 0x00, 0x00, 0x01,
        0x00, 0x00, 0x00, 0x78, 0x00, 0x00, 0x00, 0x08, 0x00, 0x0c, 0x00, 0x04, 0x00, 0x08, 0x00,
        0x08, 0x00, 0x00, 0x00, 0x34, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x16,
        0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0xd0, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xe0, 0x3f, 0x00,
        0x00, 0x0a, 0x00, 0x20, 0x00, 0x04, 0x00, 0x0c, 0x00, 0x14, 0x00, 0x0a, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x40, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00,
        0x00, 0x00, 0x6c, 0x69, 0x64, 0x61, 0x72, 0x00, 0x00, 0x00,
    ];
    // As above, but with quaternion `w` at its schema default of 1.0, which flatc elides
    // from the wire.
    const GOLDEN_DEFAULT_W: &[u8] = &[
        0x14, 0x00, 0x00, 0x00, 0x10, 0x00, 0x20, 0x00, 0x04, 0x00, 0x0c, 0x00, 0x10, 0x00, 0x14,
        0x00, 0x18, 0x00, 0x1c, 0x00, 0x10, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x22, 0x00,
        0x00, 0x00, 0xdc, 0x00, 0x00, 0x00, 0x8c, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x18,
        0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f,
        0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x40, 0x40, 0x03, 0x00, 0x00, 0x00, 0x4c, 0x00, 0x00,
        0x00, 0x28, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xea, 0xff, 0xff, 0xff, 0x00, 0x00,
        0x00, 0x07, 0x08, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x7a,
        0x00, 0x0a, 0x00, 0x10, 0x00, 0x08, 0x00, 0x0c, 0x00, 0x07, 0x00, 0x0a, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x07, 0x08, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
        0x00, 0x79, 0x00, 0x0a, 0x00, 0x0c, 0x00, 0x08, 0x00, 0x00, 0x00, 0x07, 0x00, 0x0a, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x04, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x78,
        0x00, 0x00, 0x00, 0x08, 0x00, 0x0e, 0x00, 0x04, 0x00, 0x08, 0x00, 0x08, 0x00, 0x00, 0x00,
        0x28, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x0e, 0x00, 0x04,
        0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xd0, 0x3f, 0x00, 0x00,
        0x0a, 0x00, 0x20, 0x00, 0x04, 0x00, 0x0c, 0x00, 0x14, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x40, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00,
        0x00, 0x6c, 0x69, 0x64, 0x61, 0x72, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn test_decodes_flatc_golden_buffer() {
        let cloud = decode_point_cloud(GOLDEN_FULL).unwrap();
        assert_eq!(cloud.timestamp, Some(Timestamp::new(12, 34)));
        assert_eq!(cloud.frame_id, "lidar");
        assert_eq!(cloud.point_stride, 12);
        assert_eq!(cloud.data.as_ref(), {
            let mut data = Vec::new();
            for c in [1.0f32, 2.0, 3.0] {
                data.extend_from_slice(&c.to_le_bytes());
            }
            data
        });
        assert_eq!(
            cloud.fields,
            [("x", 0), ("y", 4), ("z", 8)]
                .into_iter()
                .map(|(name, offset)| PackedElementField {
                    name: name.to_string(),
                    offset,
                    r#type: 7, // FLOAT32
                })
                .collect::<Vec<_>>()
        );
        let pose = cloud.pose.expect("pose should be present");
        let position = pose.position.expect("position should be present");
        assert_eq!((position.x, position.y, position.z), (1.0, 2.0, 3.0));
        let orientation = pose.orientation.expect("orientation should be present");
        assert_eq!(
            (orientation.x, orientation.y, orientation.z, orientation.w),
            (0.25, 0.0, 0.0, 0.5)
        );
    }

    #[test]
    fn test_flatc_golden_buffer_default_w() {
        let cloud = decode_point_cloud(GOLDEN_DEFAULT_W).unwrap();
        let orientation = cloud.pose.unwrap().orientation.unwrap();
        assert_eq!(orientation.w, 1.0);
    }
}
