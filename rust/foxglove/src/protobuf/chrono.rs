//! ProtobufField implementations for chrono types.

use super::ProtobufField;
use crate::convert::SaturatingFrom;
use crate::schemas_wkt::{Duration, Timestamp};
use prost_types::field_descriptor_proto::Type as ProstFieldType;

impl ProtobufField for ::chrono::DateTime<::chrono::Utc> {
    fn field_type() -> ProstFieldType {
        <Timestamp as ProtobufField>::field_type()
    }

    fn wire_type() -> u32 {
        <Timestamp as ProtobufField>::wire_type()
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        let ts = Timestamp::saturating_from(*self);
        <Timestamp as ProtobufField>::write(&ts, buf);
    }

    fn type_name() -> Option<String> {
        <Timestamp as ProtobufField>::type_name()
    }

    fn file_descriptor() -> Option<prost_types::FileDescriptorProto> {
        <Timestamp as ProtobufField>::file_descriptor()
    }

    fn encoded_len(&self) -> usize {
        let ts = Timestamp::saturating_from(*self);
        <Timestamp as ProtobufField>::encoded_len(&ts)
    }
}

impl ProtobufField for ::chrono::TimeDelta {
    fn field_type() -> ProstFieldType {
        <Duration as ProtobufField>::field_type()
    }

    fn wire_type() -> u32 {
        <Duration as ProtobufField>::wire_type()
    }

    fn write(&self, buf: &mut impl bytes::BufMut) {
        let dur = Duration::saturating_from(*self);
        <Duration as ProtobufField>::write(&dur, buf);
    }

    fn type_name() -> Option<String> {
        <Duration as ProtobufField>::type_name()
    }

    fn file_descriptor() -> Option<prost_types::FileDescriptorProto> {
        <Duration as ProtobufField>::file_descriptor()
    }

    fn encoded_len(&self) -> usize {
        let dur = Duration::saturating_from(*self);
        <Duration as ProtobufField>::encoded_len(&dur)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::chrono::{TimeZone, Utc};

    #[test]
    fn test_datetime_type_name() {
        assert_eq!(
            <::chrono::DateTime<Utc> as ProtobufField>::type_name(),
            Some(".google.protobuf.Timestamp".to_string())
        );
    }

    #[test]
    fn test_datetime_field_type() {
        assert_eq!(
            <::chrono::DateTime<Utc> as ProtobufField>::field_type(),
            ProstFieldType::Message
        );
    }

    #[test]
    fn test_datetime_write_and_decode() {
        // Create a known timestamp: 2024-01-15 12:30:45.123456789 UTC
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 12, 30, 45).unwrap()
            + ::chrono::Duration::nanoseconds(123_456_789);

        let mut buf = bytes::BytesMut::new();
        <::chrono::DateTime<Utc> as ProtobufField>::write(&dt, &mut buf);

        // ProtobufField::write includes a length prefix for nested messages,
        // so we need to decode the length first, then the message
        use prost::Message;
        let mut slice = &buf[..];
        let len = prost::encoding::decode_varint(&mut slice).expect("decode length");
        assert_eq!(slice.len(), len as usize);
        let decoded = Timestamp::decode(slice).expect("decode failed");

        assert_eq!(decoded.sec() as i64, dt.timestamp());
        assert_eq!(decoded.nsec(), dt.timestamp_subsec_nanos());
    }

    #[test]
    fn test_datetime_encoded_len() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 12, 30, 45).unwrap();

        let mut buf = bytes::BytesMut::new();
        <::chrono::DateTime<Utc> as ProtobufField>::write(&dt, &mut buf);

        assert_eq!(
            <::chrono::DateTime<Utc> as ProtobufField>::encoded_len(&dt),
            buf.len()
        );
    }

    #[test]
    fn test_timedelta_type_name() {
        assert_eq!(
            <::chrono::TimeDelta as ProtobufField>::type_name(),
            Some(".google.protobuf.Duration".to_string())
        );
    }

    #[test]
    fn test_timedelta_write_and_decode() {
        let delta = ::chrono::TimeDelta::seconds(123) + ::chrono::TimeDelta::nanoseconds(456_789);

        let mut buf = bytes::BytesMut::new();
        <::chrono::TimeDelta as ProtobufField>::write(&delta, &mut buf);

        // ProtobufField::write includes a length prefix for nested messages,
        // so we need to decode the length first, then the message
        use prost::Message;
        let mut slice = &buf[..];
        let len = prost::encoding::decode_varint(&mut slice).expect("decode length");
        assert_eq!(slice.len(), len as usize);
        let decoded = Duration::decode(slice).expect("decode failed");

        assert_eq!(decoded.sec(), 123);
        assert_eq!(decoded.nsec(), 456_789);
    }
}
