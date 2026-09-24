//! Draco point-cloud compression.
//!
//! This module provides [`compress_point_cloud`], which encodes a
//! [`PointCloud`] into a Draco-compressed
//! [`CompressedPointCloud`] (`format = "draco"`).
//!
//! At least two of the `x`, `y`, and `z` fields are required; they are combined into a
//! single 3-component float32 POSITION attribute (what the Foxglove app's Draco decoder
//! requires), with a missing axis padded with 0.0. Every other
//! field becomes a single-component generic Draco attribute with its native numeric type:
//! integer fields are always copied losslessly, and float32 fields are quantized with the
//! same setting as positions (or copied losslessly with
//! [`DracoEncodeOptions::lossless`]).
//!
//! Two encoding methods are available (see [`DracoMethod`]). The default kd-tree encoding
//! compresses best, but reorders points and cannot encode float64 fields: a non-empty
//! cloud containing one (other than `x`/`y`/`z`, which are narrowed into the float32
//! POSITION attribute) is rejected when quantization is requested. Sequential encoding
//! ([`DracoMethod::Sequential`]) preserves point order and copies float64 fields
//! losslessly, at a lower compression ratio.

use bytes::Bytes;

use draco_core::draco_types::DataType;
use draco_core::encoder_buffer::EncoderBuffer;
use draco_core::encoder_options::EncoderOptions;
use draco_core::geometry_attribute::{GeometryAttributeType, PointAttribute};
use draco_core::point_cloud::PointCloud as DracoCloud;
use draco_core::point_cloud_encoder::PointCloudEncoder;

use draco_core::metadata::Metadata;

use crate::messages::{CompressedPointCloud, PointCloud};

/// Draco point-cloud encoding method.
///
/// Selected with [`DracoEncodeOptionsBuilder::method`]; the default is
/// [`DracoMethod::KdTree`]. Lossless options ([`DracoEncodeOptions::lossless`]) always
/// encode sequentially, since kd-tree encoding requires quantization.
///
/// Quantization applies to positions and every float32 field under both methods, and
/// integer fields are copied losslessly under both; the methods differ in point order,
/// float64 support, and compression ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DracoMethod {
    /// kd-tree encoding: the best compression ratios, but points are reordered, and
    /// float64 fields cannot be encoded (see [`DracoEncodeError::UnquantizableField`]).
    #[default]
    KdTree,
    /// Sequential encoding: preserves point order and copies float64 fields losslessly,
    /// at a lower compression ratio than kd-tree.
    Sequential,
}

impl DracoMethod {
    /// The method value Draco's `EncoderOptions` expects.
    fn code(self) -> i32 {
        match self {
            DracoMethod::Sequential => 0,
            DracoMethod::KdTree => 1,
        }
    }
}

/// The maximum supported value for [`DracoEncodeOptionsBuilder::quantization_bits`].
pub const MAX_QUANTIZATION_BITS: u8 = 30;

/// Options for Draco point-cloud encoding.
///
/// Construct with [`Default::default`] (kd-tree encoding with 12-bit quantization),
/// [`DracoEncodeOptions::builder`], or [`DracoEncodeOptions::lossless`]. Invalid settings
/// are unrepresentable: whatever options a caller holds are valid.
///
/// ```
/// use foxglove::draco::{DracoEncodeOptions, DracoMethod};
/// let kd_tree = DracoEncodeOptions::builder().quantization_bits(10).build()?;
/// let sequential = DracoEncodeOptions::builder()
///     .quantization_bits(10)
///     .method(DracoMethod::Sequential)
///     .build()?;
/// # Ok::<(), foxglove::draco::DracoEncodeError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DracoEncodeOptions {
    /// Invariant: `0` (lossless) or `1..=MAX_QUANTIZATION_BITS`, enforced by the
    /// constructors and [`DracoEncodeOptionsBuilder::build`].
    quantization_bits: u8,
    /// Invariant: [`DracoMethod::Sequential`] whenever `quantization_bits` is `0`, since
    /// kd-tree encoding requires quantization; enforced by the constructors.
    method: DracoMethod,
}

impl DracoEncodeOptions {
    /// Returns a builder for lossy options, starting from the defaults (kd-tree encoding
    /// with 12-bit quantization).
    pub fn builder() -> DracoEncodeOptionsBuilder {
        DracoEncodeOptionsBuilder::default()
    }

    /// Creates options that quantize positions to `bits` bits (lossy) with the default
    /// kd-tree encoding.
    #[deprecated(
        since = "0.28.0",
        note = "use DracoEncodeOptions::builder().quantization_bits(bits).build()"
    )]
    pub fn with_quantization_bits(bits: u8) -> Result<Self, DracoEncodeError> {
        Self::builder().quantization_bits(bits).build()
    }

    /// Creates options that encode positions as lossless float32, using the
    /// order-preserving sequential encoding (kd-tree encoding requires quantization).
    ///
    /// Lossless output provides no size reduction over the raw cloud, so on the
    /// remote-access path a channel configured with these options is delivered
    /// unmodified rather than compressed.
    pub fn lossless() -> Self {
        Self {
            quantization_bits: 0,
            method: DracoMethod::Sequential,
        }
    }

    /// Returns the configured quantization bits, or `0` for lossless encoding.
    pub fn quantization_bits(&self) -> u8 {
        self.quantization_bits
    }

    /// Returns the encoding method.
    pub fn method(&self) -> DracoMethod {
        self.method
    }

    /// Returns true if these options encode losslessly.
    pub fn is_lossless(&self) -> bool {
        self.quantization_bits == 0
    }
}

impl Default for DracoEncodeOptions {
    fn default() -> Self {
        Self {
            quantization_bits: 12,
            method: DracoMethod::KdTree,
        }
    }
}

/// Builder for lossy [`DracoEncodeOptions`], obtained from [`DracoEncodeOptions::builder`].
///
/// Settings default to kd-tree encoding with 12-bit quantization and are validated by
/// [`build`](Self::build). Lossless options are not built; use
/// [`DracoEncodeOptions::lossless`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DracoEncodeOptionsBuilder {
    quantization_bits: u8,
    method: DracoMethod,
}

impl Default for DracoEncodeOptionsBuilder {
    fn default() -> Self {
        let DracoEncodeOptions {
            quantization_bits,
            method,
        } = DracoEncodeOptions::default();
        Self {
            quantization_bits,
            method,
        }
    }
}

impl DracoEncodeOptionsBuilder {
    /// Sets the quantization bits for positions and float32 fields, between `1` and
    /// [`MAX_QUANTIZATION_BITS`] inclusive; anything else is rejected by
    /// [`build`](Self::build).
    #[must_use]
    pub fn quantization_bits(mut self, bits: u8) -> Self {
        self.quantization_bits = bits;
        self
    }

    /// Sets the encoding method.
    #[must_use]
    pub fn method(mut self, method: DracoMethod) -> Self {
        self.method = method;
        self
    }

    /// Validates the settings and builds the options.
    ///
    /// Quantization bits outside `1..=MAX_QUANTIZATION_BITS` are rejected with
    /// [`DracoEncodeError::InvalidQuantizationBits`]; for lossless encoding use
    /// [`DracoEncodeOptions::lossless`] instead of `0`.
    pub fn build(self) -> Result<DracoEncodeOptions, DracoEncodeError> {
        let Self {
            quantization_bits,
            method,
        } = self;
        if quantization_bits == 0 || quantization_bits > MAX_QUANTIZATION_BITS {
            return Err(DracoEncodeError::InvalidQuantizationBits {
                bits: quantization_bits,
            });
        }
        Ok(DracoEncodeOptions {
            quantization_bits,
            method,
        })
    }
}

/// An error during Draco point-cloud encoding.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DracoEncodeError {
    /// The requested quantization bits are outside `1..=MAX_QUANTIZATION_BITS`.
    #[error(
        "quantization_bits ({bits}) must be between 1 and {MAX_QUANTIZATION_BITS}; use \
         DracoEncodeOptions::lossless() for lossless encoding"
    )]
    InvalidQuantizationBits {
        /// The requested quantization bits.
        bits: u8,
    },
    /// The point cloud's `point_stride` is zero.
    #[error("point_stride is 0")]
    ZeroStride,
    /// The point cloud's data length is not a multiple of `point_stride`.
    #[error("data length {len} is not a multiple of point_stride {stride}")]
    MisalignedData {
        /// Length of the point cloud data in bytes.
        len: usize,
        /// The point stride in bytes.
        stride: usize,
    },
    /// The point cloud has fewer than two of the `x`, `y`, and `z` fields.
    #[error("point cloud has fewer than two of the x/y/z position fields")]
    MissingPositionFields,
    /// A field has an unsupported numeric type.
    #[error("field '{name}' has unsupported numeric type {numeric_type}")]
    UnsupportedFieldType {
        /// The field name.
        name: String,
        /// The unrecognized `PackedElementField` numeric type value.
        numeric_type: i32,
    },
    /// The point cloud has a float64 field, which the kd-tree encoder cannot encode.
    ///
    /// Emitted only for [`DracoMethod::KdTree`] with quantization requested (the options
    /// are not [lossless](DracoEncodeOptions::lossless)) on a non-empty cloud;
    /// [`DracoMethod::Sequential`] and lossless encoding copy float64 fields losslessly.
    /// The `x`/`y`/`z` position fields are exempt: they are narrowed
    /// into the float32 POSITION attribute and never become float64 attributes.
    #[error(
        "field '{name}' is float64, which the kd-tree encoder cannot quantize; use float32 \
         or integer fields, select DracoMethod::Sequential, or exclude this channel from \
         compression"
    )]
    UnquantizableField {
        /// The float64 field name.
        name: String,
    },
    /// A field extends past the end of the point stride.
    #[error("field '{name}' (offset {offset}, size {size}) exceeds stride {stride}")]
    FieldExceedsStride {
        /// The field name.
        name: String,
        /// The field's byte offset within a point.
        offset: usize,
        /// The field's size in bytes.
        size: usize,
        /// The point stride in bytes.
        stride: usize,
    },
    /// The Draco encoder failed.
    #[error("draco encode failed: {0}")]
    Encode(String),
}

/// foxglove `PackedElementField.NumericType` -> (Draco `DataType`, byte size).
fn numeric_type(t: i32) -> Option<(DataType, usize)> {
    Some(match t {
        1 => (DataType::Uint8, 1),
        2 => (DataType::Int8, 1),
        3 => (DataType::Uint16, 2),
        4 => (DataType::Int16, 2),
        5 => (DataType::Uint32, 4),
        6 => (DataType::Int32, 4),
        7 => (DataType::Float32, 4),
        8 => (DataType::Float64, 8),
        _ => return None,
    })
}

/// Read a single scalar at `bytes[off..]` of `dtype` as f32 (for the position attribute,
/// which the Foxglove Draco decoder requires to be float32).
fn read_as_f32(bytes: &[u8], off: usize, dtype: DataType) -> f32 {
    macro_rules! le {
        ($t:ty, $n:literal) => {
            <$t>::from_le_bytes(bytes[off..off + $n].try_into().unwrap())
        };
    }
    match dtype {
        DataType::Float32 => le!(f32, 4),
        DataType::Float64 => le!(f64, 8) as f32,
        DataType::Uint8 => bytes[off] as f32,
        DataType::Int8 => bytes[off] as i8 as f32,
        DataType::Uint16 => le!(u16, 2) as f32,
        DataType::Int16 => le!(i16, 2) as f32,
        DataType::Uint32 => le!(u32, 4) as f32,
        DataType::Int32 => le!(i32, 4) as f32,
        _ => 0.0,
    }
}

/// Compresses a [`PointCloud`] into a Draco-encoded [`CompressedPointCloud`].
///
/// `timestamp`, `frame_id`, and `pose` are copied from the input cloud, and `format` is set
/// to `"draco"`.
///
/// The cloud must contain at least two of the `x`, `y`, and `z` fields, which are combined
/// into a 3-component float32 POSITION attribute; a missing axis is padded with 0.0.
/// Unless the options are lossless, positions are quantized (lossy). Every other field
/// becomes a generic Draco attribute with its native numeric type: integer fields are
/// copied losslessly, and float32 fields are quantized with the same setting as positions
/// (or copied losslessly with [`DracoEncodeOptions::lossless`]).
///
/// The default kd-tree encoding reorders points and cannot encode float64 fields: when
/// quantization is requested, a non-empty cloud containing one (other than `x`/`y`/`z`)
/// is rejected with [`DracoEncodeError::UnquantizableField`]. Select
/// [`DracoMethod::Sequential`] to preserve point order and copy float64 fields losslessly
/// (at a lower compression ratio), use
/// [`DracoEncodeOptions::lossless`] to copy every field losslessly (no size reduction),
/// or convert float64 fields to float32 or integer fields.
///
/// # Example
///
/// ```no_run
/// use foxglove::draco::{compress_point_cloud, DracoEncodeOptions};
/// use foxglove::messages::PointCloud;
///
/// # fn build_cloud() -> PointCloud { unimplemented!() }
/// let cloud: PointCloud = build_cloud();
/// let compressed = compress_point_cloud(&cloud, &DracoEncodeOptions::default()).unwrap();
/// foxglove::log!("/point_cloud", compressed);
/// ```
pub fn compress_point_cloud(
    cloud: &PointCloud,
    options: &DracoEncodeOptions,
) -> Result<CompressedPointCloud, DracoEncodeError> {
    let data = encode_draco(cloud, options)?;
    Ok(CompressedPointCloud {
        timestamp: cloud.timestamp,
        frame_id: cloud.frame_id.clone(),
        pose: cloud.pose,
        data: Bytes::from(data),
        format: "draco".to_string(),
    })
}

impl PointCloud {
    /// Compresses this point cloud into a Draco-encoded [`CompressedPointCloud`].
    ///
    /// This is shorthand for [`compress_point_cloud`](crate::draco::compress_point_cloud);
    /// see its documentation for details.
    pub fn encode_draco(
        &self,
        options: &DracoEncodeOptions,
    ) -> Result<CompressedPointCloud, DracoEncodeError> {
        compress_point_cloud(self, options)
    }
}

/// Encodes the packed point buffer of `cloud` into a Draco bitstream.
fn encode_draco(
    cloud: &PointCloud,
    options: &DracoEncodeOptions,
) -> Result<Vec<u8>, DracoEncodeError> {
    struct Field {
        offset: usize,
        dtype: DataType,
        size: usize,
    }

    let stride = cloud.point_stride as usize;
    if stride == 0 {
        return Err(DracoEncodeError::ZeroStride);
    }
    if !cloud.data.len().is_multiple_of(stride) {
        return Err(DracoEncodeError::MisalignedData {
            len: cloud.data.len(),
            stride,
        });
    }
    let num_points = cloud.data.len() / stride;

    // Resolve each field's Draco type/size and validate it fits in the stride, locating
    // the x/y/z position fields along the way.
    let mut fields = Vec::with_capacity(cloud.fields.len());
    let (mut xi, mut yi, mut zi) = (None, None, None);
    for (idx, f) in cloud.fields.iter().enumerate() {
        let (dtype, size) =
            numeric_type(f.r#type).ok_or_else(|| DracoEncodeError::UnsupportedFieldType {
                name: f.name.clone(),
                numeric_type: f.r#type,
            })?;
        let offset = f.offset as usize;
        if offset + size > stride {
            return Err(DracoEncodeError::FieldExceedsStride {
                name: f.name.clone(),
                offset,
                size,
                stride,
            });
        }
        match f.name.as_str() {
            "x" => xi = Some(idx),
            "y" => yi = Some(idx),
            "z" => zi = Some(idx),
            _ => {}
        }
        fields.push(Field {
            offset,
            dtype,
            size,
        });
    }

    // x/y/z become a single 3-component float32 POSITION attribute (required by the
    // Foxglove decoder). The schema requires at least two of the three; a missing axis
    // is padded with 0.0 (2D clouds).
    let present = usize::from(xi.is_some()) + usize::from(yi.is_some()) + usize::from(zi.is_some());
    if present < 2 {
        return Err(DracoEncodeError::MissingPositionFields);
    }

    // An empty cloud has no point range to quantize over; both encoders fail on zero
    // quantized points, and empty clouds are a legitimate "nothing detected this frame"
    // signal that must round-trip (as header-sized lossless output) rather than error.
    let quantization_bits = if num_points == 0 {
        0
    } else {
        options.quantization_bits
    };

    // kd-tree encoding requires quantization, so an empty cloud (quantization disabled
    // above) is encoded sequentially whatever the options say; the options' own
    // invariant already forces sequential for lossless settings.
    let method = if quantization_bits == 0 {
        DracoMethod::Sequential
    } else {
        options.method
    };

    // The kd-tree encoder doesn't support float64 attributes, so quantized kd-tree
    // encoding of a float64 field is an error: the caller should select sequential
    // encoding (which copies float64 fields losslessly), use float32 or integer fields,
    // or skip compression. Falling back to sequential silently would change point order
    // and output size behind the caller's back. The x/y/z fields are exempt because
    // they are narrowed into the float32 POSITION attribute and never become float64
    // attributes.
    if method == DracoMethod::KdTree {
        for (idx, field) in fields.iter().enumerate() {
            if field.dtype == DataType::Float64
                && Some(idx) != xi
                && Some(idx) != yi
                && Some(idx) != zi
            {
                return Err(DracoEncodeError::UnquantizableField {
                    name: cloud.fields[idx].name.clone(),
                });
            }
        }
    }

    let mut draco_cloud = DracoCloud::new();
    draco_cloud.set_num_points(num_points);

    // POSITION attribute (float32 x/y/z).
    let mut pos = PointAttribute::new();
    pos.init(
        GeometryAttributeType::Position,
        3,
        DataType::Float32,
        false,
        num_points,
    );
    {
        let dst = pos.buffer_mut().data_mut();
        // A missing axis (`None`) is padded with 0.0.
        let coords = [xi, yi, zi].map(|i| i.map(|i| (fields[i].offset, fields[i].dtype)));
        for p in 0..num_points {
            let base = p * stride;
            for (c, coord) in coords.iter().enumerate() {
                let v = match *coord {
                    Some((off, dt)) => read_as_f32(&cloud.data, base + off, dt),
                    None => 0.0,
                };
                let o = (p * 3 + c) * 4;
                dst[o..o + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
    }
    draco_cloud.add_attribute(pos);

    // Carry every remaining field (intensity, rgb, ring, ...) through compression.
    // Draco stores data column-major, so each field becomes its own attribute: a
    // single-component column of the field's values in their native type, copied raw
    // out of the packed rows. GENERIC is Draco's type for data it attaches no meaning
    // to (as opposed to POSITION), and generic attributes are anonymous in the
    // bitstream, so each field's name rides along as a per-attribute "name" metadata
    // entry; the app's decoder reads it to rebuild the named fields (e.g. to color by
    // intensity). `add_attribute` assigns unique ids sequentially, overwriting any id
    // set beforehand, so the metadata must be keyed by the id it returns.
    for (idx, field) in fields.iter().enumerate() {
        if Some(idx) == xi || Some(idx) == yi || Some(idx) == zi {
            continue;
        }
        let mut attr = PointAttribute::new();
        attr.init(
            GeometryAttributeType::Generic,
            1,
            field.dtype,
            false,
            num_points,
        );
        let dst = attr.buffer_mut().data_mut();
        let sz = field.size;
        for p in 0..num_points {
            let s = p * stride + field.offset;
            dst[p * sz..p * sz + sz].copy_from_slice(&cloud.data[s..s + sz]);
        }
        let attr_id = draco_cloud.add_attribute(attr);

        let name = &cloud.fields[idx].name;
        let mut attr_metadata = Metadata::new();
        // `set_string` rejects empty values; a nameless field simply gets no metadata.
        if attr_metadata.set_string("name", name).is_ok() {
            draco_cloud
                .metadata_or_insert()
                .set_attribute_metadata(attr_id as u32, attr_metadata);
        }
    }

    let mut encoder_options = EncoderOptions::new();
    encoder_options.set_encoding_method(method.code());
    if quantization_bits > 0 {
        // A global setting applies to every attribute without its own, so positions and
        // extra float32 fields are quantized alike under both methods: the kd-tree
        // encoder fails on an unquantized float32 attribute, and the sequential encoder
        // quantizes only float32 attributes, copying integer and float64 attributes
        // losslessly whatever the setting.
        encoder_options.set_global_int("quantization_bits", i32::from(quantization_bits));
    }

    let mut encoder = PointCloudEncoder::new();
    encoder.set_point_cloud(draco_cloud);
    let mut buffer = EncoderBuffer::new();
    encoder
        .encode(&encoder_options, &mut buffer)
        .map_err(|e| DracoEncodeError::Encode(e.to_string()))?;
    Ok(buffer.data().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{PackedElementField, packed_element_field::NumericType};

    use draco_core::decoder_buffer::DecoderBuffer;
    use draco_core::point_cloud_decoder::PointCloudDecoder;

    fn field(name: &str, offset: u32, numeric_type: NumericType) -> PackedElementField {
        PackedElementField {
            name: name.to_string(),
            offset,
            r#type: numeric_type as i32,
        }
    }

    /// Builds a small cloud with float32 x/y/z and a uint16 intensity field.
    fn test_cloud() -> (PointCloud, Vec<[f32; 3]>, Vec<u16>) {
        let positions: Vec<[f32; 3]> = (0..64)
            .map(|i| {
                let f = i as f32;
                [f * 0.25, f * -0.5 + 3.0, (f * 0.125).sin() * 10.0]
            })
            .collect();
        let intensities: Vec<u16> = (0..64).map(|i| (i * 37 % 1024) as u16).collect();

        let stride = 14; // 3 * f32 + u16
        let mut data = Vec::with_capacity(positions.len() * stride);
        for (pos, intensity) in positions.iter().zip(&intensities) {
            for c in pos {
                data.extend_from_slice(&c.to_le_bytes());
            }
            data.extend_from_slice(&intensity.to_le_bytes());
        }

        let cloud = PointCloud {
            timestamp: Some(crate::messages::Timestamp::new(123, 456)),
            frame_id: "lidar".to_string(),
            pose: None,
            point_stride: stride as u32,
            fields: vec![
                field("x", 0, NumericType::Float32),
                field("y", 4, NumericType::Float32),
                field("z", 8, NumericType::Float32),
                field("intensity", 12, NumericType::Uint16),
            ],
            data: Bytes::from(data),
        };
        (cloud, positions, intensities)
    }

    /// Decodes a Draco bitstream into a raw Draco point cloud.
    fn decode_cloud(draco: &[u8]) -> DracoCloud {
        let mut decoded = DracoCloud::new();
        let mut buf = DecoderBuffer::new(draco);
        PointCloudDecoder::new()
            .decode(&mut buf, &mut decoded)
            .expect("draco decode failed");
        decoded
    }

    #[test]
    fn test_field_names_travel_in_attribute_metadata() {
        let (cloud, _, _) = test_cloud();
        let draco = encode_draco(&cloud, &DracoEncodeOptions::default()).unwrap();
        let decoded = decode_cloud(&draco);

        // POSITION is attribute 0; the intensity field is the generic attribute with the
        // sequentially assigned unique id 1, named via metadata.
        let generic = decoded
            .attribute_by_unique_id(1)
            .expect("generic attribute missing");
        assert_eq!(generic.attribute_type(), GeometryAttributeType::Generic);
        let name = decoded
            .metadata()
            .and_then(|m| m.attribute_metadata_by_unique_id(1))
            .and_then(|m| m.metadata().get_string("name"));
        assert_eq!(name, Some("intensity"));
    }

    #[test]
    fn test_leading_non_position_field() {
        // Nothing guarantees clouds start with x/y/z: intensity comes first here. The
        // position fields must still be found and combined into POSITION, and the
        // leading field must keep its own identity (unique id and metadata name).
        let positions: Vec<[f32; 3]> = (0..32)
            .map(|i| {
                let f = i as f32;
                [f, f * 2.0, f * -0.5]
            })
            .collect();
        let intensities: Vec<f32> = (0..32).map(|i| i as f32 * 0.125).collect();

        let stride = 16;
        let mut data = Vec::with_capacity(positions.len() * stride);
        for (pos, intensity) in positions.iter().zip(&intensities) {
            data.extend_from_slice(&intensity.to_le_bytes());
            for c in pos {
                data.extend_from_slice(&c.to_le_bytes());
            }
        }
        let cloud = PointCloud {
            timestamp: None,
            frame_id: "lidar".to_string(),
            pose: None,
            point_stride: stride as u32,
            fields: vec![
                field("intensity", 0, NumericType::Float32),
                field("x", 4, NumericType::Float32),
                field("y", 8, NumericType::Float32),
                field("z", 12, NumericType::Float32),
            ],
            data: Bytes::from(data),
        };

        // Lossless, which uses the order-preserving sequential encoding internally, so
        // decoded values compare exactly.
        let options = DracoEncodeOptions::lossless();
        let draco = encode_draco(&cloud, &options).unwrap();
        let decoded = decode_cloud(&draco);

        assert_eq!(decoded.num_attributes(), 2);

        // POSITION keeps unique id 0 and carries the x/y/z values.
        let pos_attr = decoded
            .attribute_by_unique_id(0)
            .expect("position attribute missing");
        assert_eq!(pos_attr.attribute_type(), GeometryAttributeType::Position);
        assert_eq!(decode_positions(&draco), positions);

        // The leading intensity field is a distinct generic attribute (unique id 1) with
        // its name in metadata and its values intact.
        let generic = decoded
            .attribute_by_unique_id(1)
            .expect("generic attribute missing");
        assert_eq!(generic.attribute_type(), GeometryAttributeType::Generic);
        let name = decoded
            .metadata()
            .and_then(|m| m.attribute_metadata_by_unique_id(1))
            .and_then(|m| m.metadata().get_string("name"));
        assert_eq!(name, Some("intensity"));

        let data = generic.buffer().data();
        let decoded_intensities: Vec<f32> = (0..decoded.num_points())
            .map(|p| f32::from_le_bytes(data[p * 4..p * 4 + 4].try_into().unwrap()))
            .collect();
        assert_eq!(decoded_intensities, intensities);
    }

    /// Decodes a Draco bitstream and returns the values of its first generic attribute,
    /// each parsed from its little-endian bytes by `read`.
    fn decode_generic<T>(draco: &[u8], read: impl Fn(&[u8]) -> T) -> Vec<T> {
        let decoded = decode_cloud(draco);
        let generic_id = decoded.named_attribute_id(GeometryAttributeType::Generic);
        assert!(generic_id >= 0, "decoded cloud has no generic attribute");
        let attr = decoded.attribute(generic_id);
        let stride = attr.byte_stride() as usize;
        let data = attr.buffer().data();
        (0..decoded.num_points())
            .map(|p| read(&data[p * stride..(p + 1) * stride]))
            .collect()
    }

    fn read_u16(bytes: &[u8]) -> u16 {
        u16::from_le_bytes(bytes[..2].try_into().unwrap())
    }

    fn read_f32(bytes: &[u8]) -> f32 {
        f32::from_le_bytes(bytes[..4].try_into().unwrap())
    }

    fn read_f64(bytes: &[u8]) -> f64 {
        f64::from_le_bytes(bytes[..8].try_into().unwrap())
    }

    /// Replaces the test cloud's uint16 intensity with `numeric_type` (float32 or
    /// float64), carrying the same values.
    fn float_intensity_cloud(numeric_type: NumericType) -> (PointCloud, Vec<[f32; 3]>, Vec<u16>) {
        let (mut cloud, positions, intensities) = test_cloud();
        let size = if numeric_type == NumericType::Float64 {
            8
        } else {
            4
        };
        cloud.fields[3] = field("intensity", 12, numeric_type);
        cloud.point_stride = 12 + size;
        let mut data = Vec::with_capacity(positions.len() * cloud.point_stride as usize);
        for (pos, intensity) in positions.iter().zip(&intensities) {
            for c in pos {
                data.extend_from_slice(&c.to_le_bytes());
            }
            if numeric_type == NumericType::Float64 {
                data.extend_from_slice(&f64::from(*intensity).to_le_bytes());
            } else {
                data.extend_from_slice(&f32::from(*intensity).to_le_bytes());
            }
        }
        cloud.data = Bytes::from(data);
        (cloud, positions, intensities)
    }

    /// The maximum per-component error of `bits`-bit quantization over `values`: the
    /// value range divided by the number of quantization steps.
    fn quantization_tolerance(values: impl IntoIterator<Item = f32>, bits: u8) -> f32 {
        let (mut min, mut max) = (f32::INFINITY, f32::NEG_INFINITY);
        for v in values {
            min = min.min(v);
            max = max.max(v);
        }
        (max - min) / (1u32 << bits) as f32
    }

    /// Decodes a Draco bitstream and returns per-point positions (sequential encoding
    /// preserves point order).
    fn decode_positions(draco: &[u8]) -> Vec<[f32; 3]> {
        let mut decoded = DracoCloud::new();
        let mut buf = DecoderBuffer::new(draco);
        PointCloudDecoder::new()
            .decode(&mut buf, &mut decoded)
            .expect("draco decode failed");

        let pos_id = decoded.named_attribute_id(GeometryAttributeType::Position);
        assert!(pos_id >= 0, "decoded cloud has no position attribute");
        let attr = decoded.attribute(pos_id);
        let stride = attr.byte_stride() as usize;
        let data = attr.buffer().data();
        (0..decoded.num_points())
            .map(|p| {
                let base = p * stride;
                std::array::from_fn(|c| {
                    f32::from_le_bytes(data[base + c * 4..base + c * 4 + 4].try_into().unwrap())
                })
            })
            .collect()
    }

    #[test]
    fn test_compress_copies_message_metadata() {
        let (cloud, _, _) = test_cloud();
        let compressed = compress_point_cloud(&cloud, &DracoEncodeOptions::default()).unwrap();
        assert_eq!(compressed.timestamp, cloud.timestamp);
        assert_eq!(compressed.frame_id, "lidar");
        assert_eq!(compressed.pose, cloud.pose);
        assert_eq!(compressed.format, "draco");
        assert!(!compressed.data.is_empty());
    }

    #[test]
    fn test_quantization_error_within_tolerance() {
        let (cloud, positions, _) = test_cloud();
        let options = DracoEncodeOptions::builder()
            .quantization_bits(14)
            .build()
            .unwrap();
        let compressed = compress_point_cloud(&cloud, &options).unwrap();
        let mut decoded = decode_positions(&compressed.data);
        assert_eq!(decoded.len(), positions.len());

        // With 14-bit quantization, the max error per component is bounded by the position
        // range divided by the number of quantization steps. kd-tree encoding reorders
        // points, so pair them up by sorting both sequences; the test cloud's x values are
        // spaced 0.25 apart, far wider than the tolerance, so sorting pairs correctly.
        let tolerance = quantization_tolerance(positions.iter().flatten().copied(), 14);
        let mut expected = positions;
        expected.sort_by(|a, b| a.partial_cmp(b).unwrap());
        decoded.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for (orig, got) in expected.iter().zip(&decoded) {
            for c in 0..3 {
                assert!(
                    (orig[c] - got[c]).abs() <= tolerance,
                    "position error too large: {} vs {}",
                    orig[c],
                    got[c],
                );
            }
        }
    }

    #[test]
    fn test_lossless_positions_with_zero_quantization_bits() {
        let (cloud, positions, _) = test_cloud();
        let options = DracoEncodeOptions::lossless();
        let compressed = compress_point_cloud(&cloud, &options).unwrap();
        let decoded = decode_positions(&compressed.data);
        assert_eq!(decoded, positions);
    }

    #[test]
    fn test_kd_tree_roundtrip_point_count() {
        let (cloud, positions, _) = test_cloud();
        let options = DracoEncodeOptions::builder()
            .quantization_bits(12)
            .build()
            .unwrap();
        let compressed = compress_point_cloud(&cloud, &options).unwrap();
        // kd-tree reorders points, so only the point count is directly comparable.
        let decoded = decode_positions(&compressed.data);
        assert_eq!(decoded.len(), positions.len());
    }

    #[test]
    fn test_kd_tree_encodes_float_extra_fields() {
        // The kd-tree encoder requires all float32 attributes to be quantized, which
        // extra fields inherit from positions.
        let (cloud, positions, _) = float_intensity_cloud(NumericType::Float32);
        let compressed = compress_point_cloud(&cloud, &DracoEncodeOptions::default()).unwrap();
        let decoded = decode_positions(&compressed.data);
        assert_eq!(decoded.len(), positions.len());
    }

    #[test]
    fn test_options_builder() {
        // The builder starts from the defaults.
        assert_eq!(DracoEncodeOptions::default().method(), DracoMethod::KdTree);
        assert_eq!(
            DracoEncodeOptions::builder().build().unwrap(),
            DracoEncodeOptions::default()
        );
        let sequential = DracoEncodeOptions::builder()
            .quantization_bits(10)
            .method(DracoMethod::Sequential)
            .build()
            .unwrap();
        assert_eq!(sequential.method(), DracoMethod::Sequential);
        assert_eq!(sequential.quantization_bits(), 10);
        assert!(!sequential.is_lossless());
        // `build` validates the bits whatever the method.
        for bits in [0, MAX_QUANTIZATION_BITS + 1] {
            assert!(matches!(
                DracoEncodeOptions::builder()
                    .quantization_bits(bits)
                    .method(DracoMethod::Sequential)
                    .build(),
                Err(DracoEncodeError::InvalidQuantizationBits { bits: b }) if b == bits
            ));
        }
        // Lossless is sequential by construction.
        assert_eq!(
            DracoEncodeOptions::lossless().method(),
            DracoMethod::Sequential
        );
    }

    #[test]
    #[allow(deprecated)]
    fn test_deprecated_with_quantization_bits_matches_builder() {
        assert_eq!(
            DracoEncodeOptions::with_quantization_bits(10).unwrap(),
            DracoEncodeOptions::builder()
                .quantization_bits(10)
                .build()
                .unwrap()
        );
        assert!(matches!(
            DracoEncodeOptions::with_quantization_bits(0),
            Err(DracoEncodeError::InvalidQuantizationBits { bits: 0 })
        ));
    }

    #[test]
    fn test_lossless_uses_sequential_fallback() {
        let (cloud, positions, _) = test_cloud();
        let options = DracoEncodeOptions::lossless();
        let compressed = compress_point_cloud(&cloud, &options).unwrap();
        // The exact, order-preserving round-trip proves the sequential fallback was used:
        // kd-tree requires quantization and reorders points.
        let decoded = decode_positions(&compressed.data);
        assert_eq!(decoded, positions);
    }

    #[test]
    fn test_sequential_quantized_preserves_point_order() {
        let (cloud, positions, intensities) = test_cloud();
        let options = DracoEncodeOptions::builder()
            .quantization_bits(14)
            .method(DracoMethod::Sequential)
            .build()
            .unwrap();
        let compressed = compress_point_cloud(&cloud, &options).unwrap();

        // Positions are quantized (so compare within tolerance) but keep their order (so
        // compare index by index, unlike the sorted kd-tree comparison).
        let decoded = decode_positions(&compressed.data);
        assert_eq!(decoded.len(), positions.len());
        let tolerance = quantization_tolerance(positions.iter().flatten().copied(), 14);
        for (orig, got) in positions.iter().zip(&decoded) {
            for c in 0..3 {
                assert!(
                    (orig[c] - got[c]).abs() <= tolerance,
                    "position error too large: {} vs {}",
                    orig[c],
                    got[c],
                );
            }
        }
        // Integer extra fields are copied losslessly, in order.
        assert_eq!(decode_generic(&compressed.data, read_u16), intensities);
    }

    #[test]
    fn test_sequential_quantizes_float_extra_fields() {
        // Float32 extra fields are quantized with the position setting under sequential
        // encoding just as under kd-tree, so the method changes point order and float64
        // support but not which fields quantization applies to.
        let (cloud, _, intensities) = float_intensity_cloud(NumericType::Float32);
        let expected: Vec<f32> = intensities.iter().map(|&i| f32::from(i)).collect();
        let options = DracoEncodeOptions::builder()
            .quantization_bits(8)
            .method(DracoMethod::Sequential)
            .build()
            .unwrap();
        let compressed = compress_point_cloud(&cloud, &options).unwrap();

        let decoded = decode_generic(&compressed.data, read_f32);
        assert_eq!(decoded.len(), expected.len());
        // Coarse 8-bit quantization over a 0..1023 range cannot reproduce every value
        // exactly; the values must nonetheless stay in order and within a step.
        assert_ne!(decoded, expected, "float32 extra field was not quantized");
        let tolerance = quantization_tolerance(expected.iter().copied(), 8);
        for (orig, got) in expected.iter().zip(&decoded) {
            assert!(
                (orig - got).abs() <= tolerance,
                "intensity error too large: {orig} vs {got}"
            );
        }
    }

    #[test]
    fn test_float64_fields_rejected_with_kd_tree() {
        let (cloud, positions, _) = float_intensity_cloud(NumericType::Float64);

        // The kd-tree encoder doesn't support float64 attributes, so quantized kd-tree
        // encoding of a float64 field is an error naming the field.
        let options = DracoEncodeOptions::builder()
            .quantization_bits(12)
            .build()
            .unwrap();
        let err = compress_point_cloud(&cloud, &options).unwrap_err();
        assert!(matches!(
            err,
            DracoEncodeError::UnquantizableField { ref name } if name == "intensity"
        ));

        // With quantization disabled, the same cloud encodes losslessly; the exact,
        // order-preserving round-trip proves the sequential path was used.
        let options = DracoEncodeOptions::lossless();
        let compressed = compress_point_cloud(&cloud, &options).unwrap();
        assert_eq!(decode_positions(&compressed.data), positions);
    }

    #[test]
    fn test_sequential_encodes_float64_fields_losslessly() {
        // Sequential encoding quantizes positions while copying float64 fields raw, so a
        // float64 field is not an error and its values round-trip exactly, in order.
        let (cloud, positions, intensities) = float_intensity_cloud(NumericType::Float64);
        let expected: Vec<f64> = intensities.iter().map(|&i| f64::from(i)).collect();
        let options = DracoEncodeOptions::builder()
            .quantization_bits(12)
            .method(DracoMethod::Sequential)
            .build()
            .unwrap();
        let compressed = compress_point_cloud(&cloud, &options).unwrap();

        assert_eq!(decode_positions(&compressed.data).len(), positions.len());
        assert_eq!(decode_generic(&compressed.data, read_f64), expected);
        // Quantized positions still shrink the cloud even though the float64 field
        // doesn't.
        assert!(
            compressed.data.len() < cloud.data.len(),
            "expected {} < {}",
            compressed.data.len(),
            cloud.data.len(),
        );
    }

    #[test]
    fn test_float64_positions_still_quantize() {
        // float64 x/y/z fields are narrowed into the float32 POSITION attribute and
        // never become float64 Draco attributes, so they must not trigger the
        // float64 rejection: the cloud quantizes normally.
        let (_, positions, intensities) = test_cloud();
        let stride = 28; // 3 * f64 + f32
        let mut data = Vec::with_capacity(positions.len() * stride);
        for (pos, intensity) in positions.iter().zip(&intensities) {
            for c in pos {
                data.extend_from_slice(&f64::from(*c).to_le_bytes());
            }
            data.extend_from_slice(&f32::from(*intensity).to_le_bytes());
        }
        let cloud = PointCloud {
            timestamp: None,
            frame_id: "lidar".to_string(),
            pose: None,
            point_stride: stride as u32,
            fields: vec![
                field("x", 0, NumericType::Float64),
                field("y", 8, NumericType::Float64),
                field("z", 16, NumericType::Float64),
                field("intensity", 24, NumericType::Float32),
            ],
            data: Bytes::from(data),
        };

        let options = DracoEncodeOptions::builder()
            .quantization_bits(12)
            .build()
            .unwrap();
        let compressed = compress_point_cloud(&cloud, &options).unwrap();
        assert_eq!(decode_positions(&compressed.data).len(), positions.len());
    }

    #[test]
    fn test_extra_field_values_roundtrip() {
        // Lossless encoding preserves point order, so extra field values compare exactly.
        let (cloud, _, intensities) = test_cloud();
        let options = DracoEncodeOptions::lossless();
        let compressed = compress_point_cloud(&cloud, &options).unwrap();
        assert_eq!(decode_generic(&compressed.data, read_u16), intensities);
    }

    #[test]
    fn test_kd_tree_extra_field_values_roundtrip() {
        // Integer extra fields are copied losslessly under kd-tree; only point order changes.
        let (cloud, _, intensities) = test_cloud();
        let compressed = compress_point_cloud(&cloud, &DracoEncodeOptions::default()).unwrap();
        let mut decoded_intensities = decode_generic(&compressed.data, read_u16);
        let mut expected = intensities;
        decoded_intensities.sort_unstable();
        expected.sort_unstable();
        assert_eq!(decoded_intensities, expected);
    }

    #[test]
    fn test_encode_draco_sugar() {
        let (cloud, _, _) = test_cloud();
        let compressed = cloud.encode_draco(&DracoEncodeOptions::default()).unwrap();
        assert_eq!(compressed.format, "draco");
        assert!(!compressed.data.is_empty());
    }

    #[test]
    fn test_quantization_bits_validated_at_construction() {
        // The boundaries of the valid range are accepted, usable, and decodable.
        let (cloud, positions, _) = test_cloud();
        DracoEncodeOptions::builder()
            .quantization_bits(1)
            .build()
            .unwrap();
        let options = DracoEncodeOptions::builder()
            .quantization_bits(MAX_QUANTIZATION_BITS)
            .build()
            .unwrap();
        let compressed = compress_point_cloud(&cloud, &options).unwrap();
        assert_eq!(decode_positions(&compressed.data).len(), positions.len());

        // ...and anything outside it is unrepresentable: rejected at construction, so
        // encoding never sees invalid options. Lossless has its own constructor.
        for bits in [0, MAX_QUANTIZATION_BITS + 1] {
            let err = DracoEncodeOptions::builder()
                .quantization_bits(bits)
                .build()
                .unwrap_err();
            assert!(matches!(
                err,
                DracoEncodeError::InvalidQuantizationBits { bits: b } if b == bits
            ));
        }
        assert!(DracoEncodeOptions::lossless().is_lossless());
        assert!(!DracoEncodeOptions::default().is_lossless());
    }

    #[test]
    fn test_zero_stride_error() {
        let (mut cloud, _, _) = test_cloud();
        cloud.point_stride = 0;
        let err = compress_point_cloud(&cloud, &DracoEncodeOptions::default()).unwrap_err();
        assert!(matches!(err, DracoEncodeError::ZeroStride));
    }

    #[test]
    fn test_misaligned_data_error() {
        let (mut cloud, _, _) = test_cloud();
        let mut data = cloud.data.to_vec();
        data.push(0);
        cloud.data = Bytes::from(data);
        let err = compress_point_cloud(&cloud, &DracoEncodeOptions::default()).unwrap_err();
        assert!(matches!(err, DracoEncodeError::MisalignedData { .. }));
    }

    #[test]
    fn test_missing_position_fields_error() {
        // Fewer than two of x/y/z is rejected.
        let (mut cloud, _, _) = test_cloud();
        cloud.fields.retain(|f| f.name != "y" && f.name != "z");
        let err = compress_point_cloud(&cloud, &DracoEncodeOptions::default()).unwrap_err();
        assert!(matches!(err, DracoEncodeError::MissingPositionFields));
    }

    #[test]
    fn test_zero_point_cloud_roundtrip() {
        // An empty cloud is a legitimate "nothing detected this frame / clear the
        // display" signal and must round-trip rather than error. Quantization is
        // undefined over zero points, so empty clouds are encoded losslessly regardless
        // of the requested bits.
        let (mut cloud, _, _) = test_cloud();
        cloud.data = Bytes::new();

        let options = DracoEncodeOptions::builder()
            .quantization_bits(12)
            .build()
            .unwrap();
        let compressed = compress_point_cloud(&cloud, &options).unwrap();
        assert!(!compressed.data.is_empty());

        let decoded = decode_cloud(&compressed.data);
        assert_eq!(decoded.num_points(), 0);
        // The POSITION attribute must survive: decoders (including the app's)
        // require it even for an empty cloud.
        let pos_id = decoded.named_attribute_id(GeometryAttributeType::Position);
        assert!(pos_id >= 0, "decoded empty cloud has no position attribute");
    }

    #[test]
    fn test_two_axis_cloud_pads_missing_axis_with_zero() {
        // A cloud with only x/y fields (2D) encodes with the missing z padded to 0.0.
        let (mut cloud, positions, _) = test_cloud();
        cloud.fields.retain(|f| f.name != "z");

        // Lossless so decoded values compare exactly.
        let options = DracoEncodeOptions::lossless();
        let draco = encode_draco(&cloud, &options).unwrap();
        let expected: Vec<[f32; 3]> = positions.iter().map(|&[x, y, _]| [x, y, 0.0]).collect();
        assert_eq!(decode_positions(&draco), expected);
    }

    #[test]
    fn test_unsupported_field_type_error() {
        let (mut cloud, _, _) = test_cloud();
        cloud.fields[3].r#type = 0; // Unknown
        let err = compress_point_cloud(&cloud, &DracoEncodeOptions::default()).unwrap_err();
        assert!(matches!(
            err,
            DracoEncodeError::UnsupportedFieldType {
                numeric_type: 0,
                ..
            }
        ));
    }

    #[test]
    fn test_field_exceeds_stride_error() {
        let (mut cloud, _, _) = test_cloud();
        cloud.fields[3].offset = 13; // uint16 at offset 13 exceeds the 14-byte stride
        let err = compress_point_cloud(&cloud, &DracoEncodeOptions::default()).unwrap_err();
        assert!(matches!(err, DracoEncodeError::FieldExceedsStride { .. }));
    }

    #[test]
    fn test_compression_reduces_size() {
        let (cloud, _, _) = test_cloud();
        let compressed = compress_point_cloud(&cloud, &DracoEncodeOptions::default()).unwrap();
        assert!(
            compressed.data.len() < cloud.data.len(),
            "expected {} < {}",
            compressed.data.len(),
            cloud.data.len(),
        );
    }
}
