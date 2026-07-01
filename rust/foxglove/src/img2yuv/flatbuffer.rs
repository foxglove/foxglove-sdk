//! FlatBuffer message decoder.

use std::str::FromStr;

use flatbuffers::{
    Follow, ForwardsUOffset, InvalidFlatbuffer, Table, VOffsetT, Vector, Verifiable, Verifier,
};

use crate::messages::Timestamp;

use super::{
    Compression, Endian, Image, ImageMessage, RawImageEncoding, UnknownCompressionError,
    UnknownEncodingError,
};

/// An error that occurs while decoding a FlatBuffer message.
#[derive(Debug, thiserror::Error)]
pub enum FlatbufferDecodeError {
    /// The buffer is not a valid FlatBuffer.
    #[error("invalid flatbuffer: {0}")]
    Invalid(#[from] InvalidFlatbuffer),
    /// The timestamp cannot be represented (excess nanoseconds overflow the seconds field).
    #[error("timestamp out of range")]
    InvalidTimestamp,
    /// Unknown raw image encoding.
    #[error(transparent)]
    UnknownEncoding(#[from] UnknownEncodingError),
    /// Unknown compression codec.
    #[error(transparent)]
    UnknownCompression(#[from] UnknownCompressionError),
}

// VTable byte offsets for the fields, derived from each table's field ids in the `.fbs` schema.
// Field id `n` lives at vtable byte offset `4 + 2 * n`.
mod compressed {
    use flatbuffers::VOffsetT;
    pub(super) const VT_TIMESTAMP: VOffsetT = 4;
    pub(super) const VT_FRAME_ID: VOffsetT = 6;
    pub(super) const VT_DATA: VOffsetT = 8;
    pub(super) const VT_FORMAT: VOffsetT = 10;
}
mod raw {
    use flatbuffers::VOffsetT;
    pub(super) const VT_TIMESTAMP: VOffsetT = 4;
    pub(super) const VT_FRAME_ID: VOffsetT = 6;
    pub(super) const VT_WIDTH: VOffsetT = 8;
    pub(super) const VT_HEIGHT: VOffsetT = 10;
    pub(super) const VT_ENCODING: VOffsetT = 12;
    pub(super) const VT_STEP: VOffsetT = 14;
    pub(super) const VT_DATA: VOffsetT = 16;
}

/// Verification marker for the `foxglove.CompressedImage` table.
///
/// Follows to a raw [`Table`]; field types are checked by [`Verifiable`] so the hand-written
/// accessors below can read the variable-length fields safely.
struct CompressedImageTable;
impl<'a> Follow<'a> for CompressedImageTable {
    type Inner = Table<'a>;
    unsafe fn follow(buf: &'a [u8], loc: usize) -> Self::Inner {
        unsafe { Table::follow(buf, loc) }
    }
}
impl Verifiable for CompressedImageTable {
    fn run_verifier(v: &mut Verifier, pos: usize) -> Result<(), InvalidFlatbuffer> {
        v.visit_table(pos)?
            .visit_field::<ForwardsUOffset<&str>>("frame_id", compressed::VT_FRAME_ID, false)?
            .visit_field::<ForwardsUOffset<Vector<u8>>>("data", compressed::VT_DATA, false)?
            .visit_field::<ForwardsUOffset<&str>>("format", compressed::VT_FORMAT, false)?
            .finish();
        Ok(())
    }
}

/// Verification marker for the `foxglove.RawImage` table.
struct RawImageTable;
impl<'a> Follow<'a> for RawImageTable {
    type Inner = Table<'a>;
    unsafe fn follow(buf: &'a [u8], loc: usize) -> Self::Inner {
        unsafe { Table::follow(buf, loc) }
    }
}
impl Verifiable for RawImageTable {
    fn run_verifier(v: &mut Verifier, pos: usize) -> Result<(), InvalidFlatbuffer> {
        v.visit_table(pos)?
            .visit_field::<ForwardsUOffset<&str>>("frame_id", raw::VT_FRAME_ID, false)?
            .visit_field::<u32>("width", raw::VT_WIDTH, false)?
            .visit_field::<u32>("height", raw::VT_HEIGHT, false)?
            .visit_field::<ForwardsUOffset<&str>>("encoding", raw::VT_ENCODING, false)?
            .visit_field::<u32>("step", raw::VT_STEP, false)?
            .visit_field::<ForwardsUOffset<Vector<u8>>>("data", raw::VT_DATA, false)?
            .finish();
        Ok(())
    }
}

/// Reads a little-endian `u32` at `loc`, or `None` if out of bounds.
fn read_u32_le(buf: &[u8], loc: usize) -> Option<u32> {
    buf.get(loc..loc + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

/// Reads the inline `foxglove.Time` struct at the given vtable slot.
///
/// Returns `None` if the field is absent (all FlatBuffer table fields are optional). The Time
/// struct is two little-endian `u32`s (`sec`, `nsec`); FlatBuffer verification covers the table and
/// its variable-length fields but not inline structs, so this read is bounds-checked and rejects
/// out-of-range timestamps rather than clamping them.
fn read_timestamp(
    table: &Table,
    slot: VOffsetT,
) -> Result<Option<Timestamp>, FlatbufferDecodeError> {
    let offset = table.vtable().get(slot);
    if offset == 0 {
        return Ok(None);
    }
    let loc = table.loc() + offset as usize;
    let buf = table.buf();
    let sec = read_u32_le(buf, loc).ok_or(FlatbufferDecodeError::InvalidTimestamp)?;
    let nsec = read_u32_le(buf, loc + 4).ok_or(FlatbufferDecodeError::InvalidTimestamp)?;
    Timestamp::new_checked(sec, nsec)
        .map(Some)
        .ok_or(FlatbufferDecodeError::InvalidTimestamp)
}

/// Decodes a FlatBuffer-encoded `foxglove.CompressedImage`.
pub fn decode_compressed_image(data: &[u8]) -> Result<ImageMessage<'_>, FlatbufferDecodeError> {
    let table = flatbuffers::root::<CompressedImageTable>(data)?;
    // Safety: the fields were verified to have these types by `CompressedImageTable`.
    let frame_id = unsafe { table.get::<ForwardsUOffset<&str>>(compressed::VT_FRAME_ID, Some("")) }
        .unwrap_or("");
    let format = unsafe { table.get::<ForwardsUOffset<&str>>(compressed::VT_FORMAT, Some("")) }
        .unwrap_or("");
    let bytes = unsafe { table.get::<ForwardsUOffset<Vector<u8>>>(compressed::VT_DATA, None) }
        .map(|v| v.bytes())
        .unwrap_or(&[]);
    let compression = Compression::from_str(format)?;
    Ok(ImageMessage {
        timestamp: read_timestamp(&table, compressed::VT_TIMESTAMP)?,
        frame_id: frame_id.to_string(),
        image: Image::Compressed(super::CompressedImage {
            compression,
            data: bytes.into(),
        }),
    })
}

/// Decodes a FlatBuffer-encoded `foxglove.RawImage`.
pub fn decode_raw_image(data: &[u8]) -> Result<ImageMessage<'_>, FlatbufferDecodeError> {
    let table = flatbuffers::root::<RawImageTable>(data)?;
    // Safety: the fields were verified to have these types by `RawImageTable`.
    let frame_id =
        unsafe { table.get::<ForwardsUOffset<&str>>(raw::VT_FRAME_ID, Some("")) }.unwrap_or("");
    let encoding =
        unsafe { table.get::<ForwardsUOffset<&str>>(raw::VT_ENCODING, Some("")) }.unwrap_or("");
    let width = unsafe { table.get::<u32>(raw::VT_WIDTH, Some(0)) }.unwrap_or(0);
    let height = unsafe { table.get::<u32>(raw::VT_HEIGHT, Some(0)) }.unwrap_or(0);
    let step = unsafe { table.get::<u32>(raw::VT_STEP, Some(0)) }.unwrap_or(0);
    let bytes = unsafe { table.get::<ForwardsUOffset<Vector<u8>>>(raw::VT_DATA, None) }
        .map(|v| v.bytes())
        .unwrap_or(&[]);
    // Pixel values in Foxglove RawImage messages are always little-endian.
    let encoding = RawImageEncoding::parse_endian(encoding, Endian::Little)?;
    Ok(ImageMessage {
        timestamp: read_timestamp(&table, raw::VT_TIMESTAMP)?,
        frame_id: frame_id.to_string(),
        image: Image::Raw(super::RawImage {
            encoding,
            width,
            height,
            stride: step,
            data: bytes.into(),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flatbuffers::{FlatBufferBuilder, PushAlignment};

    /// Writes a `foxglove.Time` inline struct (sec, nsec). Structs are emitted directly into the
    /// parent table via `push_slot_always`, so this returns the encoded bytes for the caller.
    fn time_bytes(sec: u32, nsec: u32) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0..4].copy_from_slice(&sec.to_le_bytes());
        buf[4..8].copy_from_slice(&nsec.to_le_bytes());
        buf
    }

    fn build_compressed(frame_id: &str, format: &str, data: &[u8], sec: u32, nsec: u32) -> Vec<u8> {
        let mut fbb = FlatBufferBuilder::new();
        let frame_id = fbb.create_string(frame_id);
        let format = fbb.create_string(format);
        let data = fbb.create_vector(data);
        let ts = time_bytes(sec, nsec);
        let start = fbb.start_table();
        // Inline struct field (timestamp) is written as a fixed-size struct in the table.
        fbb.push_slot_always(compressed::VT_TIMESTAMP, FbTime(ts));
        fbb.push_slot_always(compressed::VT_FRAME_ID, frame_id);
        fbb.push_slot_always(compressed::VT_DATA, data);
        fbb.push_slot_always(compressed::VT_FORMAT, format);
        let root = fbb.end_table(start);
        fbb.finish(root, None);
        fbb.finished_data().to_vec()
    }

    fn build_raw(
        frame_id: &str,
        encoding: &str,
        width: u32,
        height: u32,
        step: u32,
        data: &[u8],
        timestamp: Option<(u32, u32)>,
    ) -> Vec<u8> {
        let mut fbb = FlatBufferBuilder::new();
        let frame_id = fbb.create_string(frame_id);
        let encoding = fbb.create_string(encoding);
        let data = fbb.create_vector(data);
        let start = fbb.start_table();
        if let Some((sec, nsec)) = timestamp {
            fbb.push_slot_always(raw::VT_TIMESTAMP, FbTime(time_bytes(sec, nsec)));
        }
        fbb.push_slot_always(raw::VT_FRAME_ID, frame_id);
        fbb.push_slot::<u32>(raw::VT_WIDTH, width, 0);
        fbb.push_slot::<u32>(raw::VT_HEIGHT, height, 0);
        fbb.push_slot_always(raw::VT_ENCODING, encoding);
        fbb.push_slot::<u32>(raw::VT_STEP, step, 0);
        fbb.push_slot_always(raw::VT_DATA, data);
        let root = fbb.end_table(start);
        fbb.finish(root, None);
        fbb.finished_data().to_vec()
    }

    /// A `foxglove.Time` struct as a pushable fixed-size value (8 bytes).
    #[derive(Clone, Copy)]
    struct FbTime([u8; 8]);
    impl flatbuffers::Push for FbTime {
        type Output = FbTime;
        unsafe fn push(&self, dst: &mut [u8], _written_len: usize) {
            dst[..8].copy_from_slice(&self.0);
        }
        fn size() -> usize {
            8
        }
        fn alignment() -> PushAlignment {
            // foxglove.Time is two u32 fields, so it aligns to 4.
            PushAlignment::new(4)
        }
    }

    #[test]
    #[cfg(feature = "img2yuv-png")]
    fn test_decode_compressed_image() {
        let buf = build_compressed("camera", "png", &[0, 1, 2, 3], 100, 200);
        let msg = decode_compressed_image(&buf).unwrap();
        assert_eq!(msg.frame_id, "camera");
        assert_eq!(msg.timestamp.unwrap().total_nanos(), 100_000_000_200);
        match msg.image {
            Image::Compressed(image) => {
                assert_eq!(image.compression, Compression::Png);
                assert_eq!(&*image.data, &[0, 1, 2, 3]);
            }
            other => panic!("expected compressed image, got {other:?}"),
        }
    }

    #[test]
    fn test_decode_raw_image() {
        let buf = build_raw("frame", "mono8", 2, 1, 2, &[0, 1], Some((1, 2)));
        let msg = decode_raw_image(&buf).unwrap();
        assert_eq!(msg.frame_id, "frame");
        assert_eq!(msg.timestamp.unwrap().total_nanos(), 1_000_000_002);
        match msg.image {
            Image::Raw(image) => {
                assert_eq!(image.encoding, RawImageEncoding::Mono8);
                assert_eq!(image.width, 2);
                assert_eq!(image.height, 1);
                assert_eq!(image.stride, 2);
                assert_eq!(&*image.data, &[0, 1]);
            }
            other => panic!("expected raw image, got {other:?}"),
        }
    }

    #[test]
    fn test_decode_unknown_encoding() {
        let buf = build_raw("frame", "not-a-real-encoding", 1, 1, 1, &[0], Some((1, 2)));
        let err = decode_raw_image(&buf).unwrap_err();
        assert!(matches!(err, FlatbufferDecodeError::UnknownEncoding(_)));
    }

    #[test]
    fn test_decode_absent_timestamp_is_none() {
        let buf = build_raw("frame", "mono8", 2, 1, 2, &[0, 1], None);
        let msg = decode_raw_image(&buf).unwrap();
        assert_eq!(msg.timestamp, None);
    }

    #[test]
    fn test_decode_rejects_overflowing_timestamp() {
        // Excess nanoseconds carry into seconds, overflowing the u32 seconds field.
        let buf = build_raw(
            "frame",
            "mono8",
            2,
            1,
            2,
            &[0, 1],
            Some((u32::MAX, 1_000_000_000)),
        );
        let err = decode_raw_image(&buf).unwrap_err();
        assert!(matches!(err, FlatbufferDecodeError::InvalidTimestamp));
    }

    #[test]
    fn test_decode_garbage() {
        let err = decode_raw_image(&[0xff, 0xff, 0xff, 0xff]).unwrap_err();
        assert!(matches!(err, FlatbufferDecodeError::Invalid(_)));
    }
}
