//! Interfaces for working with Protocol Buffers.
use prost_types::field_descriptor_proto::Type as ProstFieldType;

/// Serializes a Protocol Buffers FileDescriptorSet to a byte vector.
///
/// This function encodes the provided FileDescriptorSet message into its binary
/// protobuf representation, which can be used for schema exchange and message
/// type definitions in Foxglove.
///
/// # Arguments
///
/// * `file_descriptor_set` - A reference to the Protocol Buffers FileDescriptorSet to serialize
///
/// # Returns
///
/// A `Vec<u8>` containing the binary protobuf encoding of the FileDescriptorSet
#[doc(hidden)]
pub fn prost_file_descriptor_set_to_vec(
    file_descriptor_set: &prost_types::FileDescriptorSet,
) -> Vec<u8> {
    use prost::Message;
    file_descriptor_set.encode_to_vec()
}

/// Encodes a u64 value as a varint and writes it to the buffer.
///
/// Requires up to 10 bytes of buffer space.
#[doc(hidden)]
pub fn encode_varint(value: u64, buf: &mut impl bytes::BufMut) {
    prost::encoding::encode_varint(value, buf);
}

/// Returns the encoded length of a value to be written with [encode_varint].
#[doc(hidden)]
pub fn encoded_len_varint(value: u64) -> usize {
    prost::encoding::encoded_len_varint(value)
}

/// The `ProtobufField` trait defines the interface for types that can be serialized to Protocol
/// Buffer format.
///
/// This trait is automatically implemented for custom types when using the `#[derive(Encode)]`
/// attribute. It provides the necessary methods to serialize data according to Protocol Buffer
/// encoding rules and generate appropriate Protocol Buffer schema information.
///
/// It supports signed and unsigned integer types, floating point, boolean, string, bytes, and
/// repeated fields. Signed integers are encoded using sint32 or sint64.
///
/// # Usage
///
/// This trait is typically implemented automatically by using the `#[derive(Encode)]` attribute
/// on your custom types:
///
/// ```rust
/// #[derive(foxglove::Encode)]
/// struct MyMessage {
///     number: u64,
///     text: String,
/// }
/// ```
pub trait ProtobufField {
    /// Returns the Protocol Buffer field type that corresponds to this Rust type.
    fn field_type() -> ProstFieldType;

    /// Returns the Protocol Buffer wire type for this Rust type
    fn wire_type() -> u32;

    /// Writes a field with its tag (field number and wire type) to the buffer.
    ///
    /// You must choose a valid field number (unique, within the max, and not reserved).
    ///
    /// The default implementation writes the tag followed by the field content.
    fn write_tagged(&self, field_number: u32, buf: &mut impl bytes::BufMut) {
        let tag = (field_number << 3) | Self::wire_type();
        prost::encoding::encode_varint(tag as u64, buf);
        self.write(buf);
    }

    /// Writes the field content to the output buffer according to Protocol Buffer encoding rules.
    fn write(&self, buf: &mut impl bytes::BufMut);

    /// Returns the type name for the type.
    ///
    /// For complex types (messages, enums) this should return the type name. For primitive types
    /// this should return None (the default).
    fn type_name() -> Option<String> {
        None
    }

    /// If this trait is implemented on an Enum type, this returns the enum descriptor for the type.
    fn enum_descriptor() -> Option<prost_types::EnumDescriptorProto> {
        None
    }

    /// If this trait is implemented on a struct type, this returns the message descriptor for the type.
    fn message_descriptor() -> Option<prost_types::DescriptorProto> {
        None
    }

    /// Indicates the type represents a repeated field (like a Vec).
    ///
    /// By default, fields are not repeated.
    fn repeating() -> bool {
        false
    }

    /// The length of the field to be written, in bytes.
    fn encoded_len(&self) -> usize;
}

impl ProtobufField for u64 {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Uint64
    }

    fn wire_type() -> u32 {
        prost::encoding::WireType::Varint as u32
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        encode_varint(*self, buf);
    }

    fn encoded_len(&self) -> usize {
        prost::encoding::encoded_len_varint(*self)
    }
}

impl ProtobufField for u32 {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Uint32
    }

    fn wire_type() -> u32 {
        prost::encoding::WireType::Varint as u32
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        encode_varint((*self).into(), buf);
    }

    fn encoded_len(&self) -> usize {
        prost::encoding::encoded_len_varint(*self as u64)
    }
}

impl ProtobufField for u16 {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Uint32
    }

    fn wire_type() -> u32 {
        prost::encoding::WireType::Varint as u32
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        encode_varint((*self).into(), buf);
    }

    fn encoded_len(&self) -> usize {
        prost::encoding::encoded_len_varint(*self as u64)
    }
}

impl ProtobufField for u8 {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Uint32
    }

    fn wire_type() -> u32 {
        prost::encoding::WireType::Varint as u32
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        encode_varint((*self).into(), buf);
    }

    fn encoded_len(&self) -> usize {
        prost::encoding::encoded_len_varint(*self as u64)
    }
}

impl ProtobufField for i64 {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Sint64
    }

    fn wire_type() -> u32 {
        prost::encoding::WireType::Varint as u32
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        // https://protobuf.dev/programming-guides/encoding/#signed-ints
        let n = *self as i128;
        let encoded = ((n << 1) ^ (n >> 63)) as u64;
        encode_varint(encoded, buf);
    }

    fn encoded_len(&self) -> usize {
        let n = *self as i128;
        let encoded = ((n << 1) ^ (n >> 63)) as u64;
        prost::encoding::encoded_len_varint(encoded)
    }
}

impl ProtobufField for i32 {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Sint32
    }

    fn wire_type() -> u32 {
        prost::encoding::WireType::Varint as u32
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        // https://protobuf.dev/programming-guides/encoding/#signed-ints
        let n = *self as i64;
        let encoded = ((n << 1) ^ (n >> 31)) as u64;
        encode_varint(encoded, buf);
    }

    fn encoded_len(&self) -> usize {
        let n = *self as i64;
        let encoded = ((n << 1) ^ (n >> 31)) as u64;
        prost::encoding::encoded_len_varint(encoded)
    }
}

impl ProtobufField for i16 {
    fn field_type() -> ProstFieldType {
        <i32 as ProtobufField>::field_type()
    }

    fn wire_type() -> u32 {
        <i32 as ProtobufField>::wire_type()
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        // https://protobuf.dev/programming-guides/encoding/#signed-ints
        let n = *self as i32;
        let encoded = ((n << 1) ^ (n >> 15)) as u64;
        encode_varint(encoded, buf);
    }

    fn encoded_len(&self) -> usize {
        let n = *self as i32;
        let encoded = ((n << 1) ^ (n >> 15)) as u64;
        prost::encoding::encoded_len_varint(encoded)
    }
}

impl ProtobufField for i8 {
    fn field_type() -> ProstFieldType {
        <i32 as ProtobufField>::field_type()
    }

    fn wire_type() -> u32 {
        <i32 as ProtobufField>::wire_type()
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        // https://protobuf.dev/programming-guides/encoding/#signed-ints
        let n = *self as i16;
        let encoded = ((n << 1) ^ (n >> 7)) as u64;
        encode_varint(encoded, buf);
    }

    fn encoded_len(&self) -> usize {
        let n = *self as i16;
        let encoded = ((n << 1) ^ (n >> 7)) as u64;
        prost::encoding::encoded_len_varint(encoded)
    }
}

impl ProtobufField for bool {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Bool
    }

    fn wire_type() -> u32 {
        prost::encoding::WireType::Varint as u32
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        buf.put_u8(*self as u8);
    }

    fn encoded_len(&self) -> usize {
        1
    }
}

impl ProtobufField for f32 {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Float
    }

    fn wire_type() -> u32 {
        prost::encoding::WireType::ThirtyTwoBit as u32
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        buf.put_f32_le(*self);
    }

    fn encoded_len(&self) -> usize {
        4 // f32
    }
}

impl ProtobufField for f64 {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Double
    }

    fn wire_type() -> u32 {
        prost::encoding::WireType::SixtyFourBit as u32
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        buf.put_f64_le(*self);
    }

    fn encoded_len(&self) -> usize {
        8 // f64
    }
}

// Implement ProtobufField for String that serializes the value in protobuf format
impl ProtobufField for String {
    fn field_type() -> ProstFieldType {
        ProstFieldType::String
    }

    fn wire_type() -> u32 {
        prost::encoding::WireType::LengthDelimited as u32
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        // Write the length as a varint, followed by the data
        prost::encoding::encode_length_delimiter(self.len(), buf).expect("Failed to write string");
        buf.put_slice(self.as_bytes());
    }

    fn encoded_len(&self) -> usize {
        let delim_len = prost::encoding::length_delimiter_len(self.len());
        delim_len + self.len()
    }
}

// Implement ProtobufField for &str, which delegates to String's implementation
impl ProtobufField for &str {
    fn field_type() -> ProstFieldType {
        <String as ProtobufField>::field_type()
    }

    fn wire_type() -> u32 {
        <String as ProtobufField>::wire_type()
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        // Write the length as a varint, followed by the data
        prost::encoding::encode_length_delimiter(self.len(), buf).expect("Failed to write str");
        buf.put_slice(self.as_bytes());
    }

    fn encoded_len(&self) -> usize {
        let delim_len = prost::encoding::length_delimiter_len(self.len());
        delim_len + self.len()
    }
}

impl ProtobufField for bytes::Bytes {
    fn field_type() -> ProstFieldType {
        ProstFieldType::Bytes
    }

    fn wire_type() -> u32 {
        prost::encoding::WireType::LengthDelimited as u32
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        // Write the length as a varint, followed by the data
        prost::encoding::encode_length_delimiter(self.len(), buf).expect("Failed to write bytes");
        buf.put_slice(self);
    }

    fn encoded_len(&self) -> usize {
        let delim_len = prost::encoding::length_delimiter_len(self.len());
        delim_len + self.len()
    }
}

// implement a protobuf field for any Vec<T> where T implements ProtobufField
impl<T> ProtobufField for Vec<T>
where
    T: ProtobufField,
{
    fn field_type() -> ProstFieldType {
        T::field_type()
    }

    fn wire_type() -> u32 {
        prost::encoding::WireType::LengthDelimited as u32
    }

    fn write_tagged(&self, field_number: u32, buf: &mut impl bytes::BufMut) {
        // non-packed repeated fields are encoded as a record for each element
        // https://protobuf.dev/programming-guides/encoding/#optional
        for value in self {
            let wire_type = T::wire_type();
            let tag = (field_number << 3) | wire_type;
            prost::encoding::encode_varint(tag as u64, buf);
            value.write(buf);
        }
    }

    fn write(&self, _buf: &mut impl bytes::BufMut) {
        panic!("Vec<T> should always be written using write_tagged");
    }

    fn repeating() -> bool {
        true
    }

    fn message_descriptor() -> Option<prost_types::DescriptorProto> {
        // The message descriptor of a vector is the message descriptor of the element type
        // the "repeating" property is set on the field that is repeating rather than the message
        // descriptor
        T::message_descriptor()
    }

    fn type_name() -> Option<String> {
        T::type_name()
    }

    fn encoded_len(&self) -> usize {
        // non-packed repeated fields
        let delim_len = prost::encoding::length_delimiter_len(self.len());
        let data_len: usize = self.iter().map(|value| value.encoded_len()).sum();
        delim_len + data_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u8_encoded_len() {
        assert_eq!(ProstFieldType::Uint32, u8::field_type());
        assert_eq!(1, u8::encoded_len(&127u8));
        assert_eq!(2, u8::encoded_len(&128u8));
    }

    #[test]
    fn test_i8_encoded_len() {
        // Zig-zag encoding
        assert_eq!(ProstFieldType::Sint32, i8::field_type());
        assert_eq!(1, (-1i8).encoded_len());
        assert_eq!(1, 1i8.encoded_len());
        assert_eq!(2, i8::MIN.encoded_len());
        assert_eq!(2, i8::MAX.encoded_len());
    }

    #[test]
    fn test_i8_write() {
        // Zig-zag encoding
        // https://protobuf.dev/programming-guides/encoding/#varints
        let cases: Vec<(i8, &[u8])> = vec![
            (-1i8, &[1]),
            (1i8, &[2]),
            (-127i8, &[253, 1]),
            (127i8, &[254, 1]),
        ];

        for (input, expected) in cases {
            let mut buf = bytes::BytesMut::new();
            i8::write(&input, &mut buf);
            assert_eq!(&buf[..], expected);

            let mut buf = bytes::BytesMut::new();
            i16::write(&(input as i16), &mut buf);
            assert_eq!(&buf[..], expected);

            let mut buf = bytes::BytesMut::new();
            i32::write(&(input as i32), &mut buf);
            assert_eq!(&buf[..], expected);

            let mut buf = bytes::BytesMut::new();
            i64::write(&(input as i64), &mut buf);
            assert_eq!(&buf[..], expected);
        }
    }

    #[test]
    fn test_i64_edges() {
        let mut buf = bytes::BytesMut::new();
        i64::write(&(i64::MAX), &mut buf);
        assert_eq!(
            &buf[..],
            &[0xfe, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01]
        );
        assert_eq!(i64::MAX.encoded_len(), 10);

        let mut buf = bytes::BytesMut::new();
        i64::write(&(i64::MIN), &mut buf);
        assert_eq!(
            &buf[..],
            &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01]
        );
        assert_eq!(i64::MIN.encoded_len(), 10);
    }

    #[test]
    fn test_write_tagged() {
        // https://protobuf.dev/programming-guides/encoding/#structure
        let mut buf = bytes::BytesMut::new();
        bool::write_tagged(&true, 1, &mut buf);
        assert_eq!(&buf[..], &[0x08, 0x01]);

        let mut buf = bytes::BytesMut::new();
        bool::write_tagged(&true, 256, &mut buf);
        assert_eq!(&buf[..], &[0x80, 0x10, 0x01]);
    }
}
