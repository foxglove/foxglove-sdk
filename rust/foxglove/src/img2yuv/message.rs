use std::borrow::Cow;

use crate::schemas::Timestamp;

use super::{
    CompressedImage, Endian, Image, RawImage, RawImageEncoding, UnknownCompressionError,
    UnknownEncodingError,
};

/// An image message.
#[derive(Debug, Clone)]
pub struct ImageMessage<'a> {
    /// The timestamp associated with the message.
    pub timestamp: Option<Timestamp>,
    /// The frame ID associated with the message.
    pub frame_id: String,
    /// The image data.
    pub image: Image<'a>,
}
impl TryFrom<crate::schemas::CompressedImage> for ImageMessage<'static> {
    type Error = UnknownCompressionError;

    fn try_from(image: crate::schemas::CompressedImage) -> std::result::Result<Self, Self::Error> {
        Ok(ImageMessage {
            timestamp: image.timestamp,
            frame_id: image.frame_id,
            image: Image::Compressed(CompressedImage {
                compression: image.format.parse()?,
                data: Cow::Owned(image.data.into()),
            }),
        })
    }
}
impl TryFrom<crate::schemas::RawImage> for ImageMessage<'static> {
    type Error = UnknownEncodingError;

    fn try_from(image: crate::schemas::RawImage) -> std::result::Result<Self, Self::Error> {
        // Pixel values in Foxglove RawImage messages are always little-endian.
        let encoding = RawImageEncoding::parse_endian(&image.encoding, Endian::Little)?;
        Ok(Self {
            timestamp: image.timestamp,
            frame_id: image.frame_id,
            image: Image::Raw(RawImage {
                encoding,
                width: image.width,
                height: image.height,
                stride: image.step,
                data: Cow::Owned(image.data.into()),
            }),
        })
    }
}
impl ImageMessage<'_> {
    /// Creates an owned image message, cloning if necessary.
    pub fn into_owned(self) -> ImageMessage<'static> {
        ImageMessage {
            timestamp: self.timestamp,
            frame_id: self.frame_id,
            image: self.image.into_owned(),
        }
    }
}
