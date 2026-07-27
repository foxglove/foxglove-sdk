//! JSON message decoder.

use super::{ImageMessage, UnknownCompressionError, UnknownEncodingError};

/// An error that occurs while decoding a JSON message.
#[derive(Debug, thiserror::Error)]
pub enum JsonDecodeError {
    /// Failed to parse the JSON message.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Unknown raw image encoding.
    #[error(transparent)]
    UnknownEncoding(#[from] UnknownEncodingError),
    /// Unknown compression codec.
    #[error(transparent)]
    UnknownCompression(#[from] UnknownCompressionError),
}

/// Decodes a JSON-encoded `foxglove.CompressedImage`.
pub fn decode_compressed_image(data: &[u8]) -> Result<ImageMessage<'static>, JsonDecodeError> {
    let image: crate::messages::CompressedImage = serde_json::from_slice(data)?;
    Ok(ImageMessage::try_from(image)?)
}

/// Decodes a JSON-encoded `foxglove.RawImage`.
pub fn decode_raw_image(data: &[u8]) -> Result<ImageMessage<'static>, JsonDecodeError> {
    let image: crate::messages::RawImage = serde_json::from_slice(data)?;
    Ok(ImageMessage::try_from(image)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "img2yuv-png")]
    use crate::img2yuv::Compression;
    use crate::img2yuv::{Image, RawImageEncoding};

    #[test]
    #[cfg(feature = "img2yuv-png")]
    fn test_decode_compressed_image() {
        let json = serde_json::json!({
            "timestamp": { "sec": 100, "nsec": 200 },
            "frame_id": "camera",
            "data": "AAECAw==",
            "format": "png",
        })
        .to_string();
        let msg = decode_compressed_image(json.as_bytes()).unwrap();
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
        let json = serde_json::json!({
            "timestamp": { "sec": 1, "nsec": 2 },
            "frame_id": "frame",
            "width": 2,
            "height": 1,
            "encoding": "mono8",
            "step": 2,
            "data": "AAE=",
        })
        .to_string();
        let msg = decode_raw_image(json.as_bytes()).unwrap();
        assert_eq!(msg.frame_id, "frame");
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
        let json = serde_json::json!({
            "timestamp": { "sec": 0, "nsec": 0 },
            "frame_id": "frame",
            "width": 1,
            "height": 1,
            "encoding": "not-a-real-encoding",
            "step": 1,
            "data": "AA==",
        })
        .to_string();
        let err = decode_raw_image(json.as_bytes()).unwrap_err();
        assert!(matches!(err, JsonDecodeError::UnknownEncoding(_)));
    }
}
