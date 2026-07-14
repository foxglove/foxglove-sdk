use std::{borrow::Cow, io::Cursor, str::FromStr};

use image::ImageReader;

use super::{Error, RawImageEncoding, Yuv420Buffer, raw::rgb_to_yuv420};

/// Unknown compression codec.
#[derive(Debug, thiserror::Error)]
#[error("unknown compression codec: {0}")]
pub struct UnknownCompressionError(String);

/// Image compression format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// PNG image compression.
    #[cfg(feature = "img2yuv-png")]
    Png,
    /// JPEG image compression.
    #[cfg(feature = "img2yuv-jpeg")]
    Jpeg,
    /// WebP image format.
    #[cfg(feature = "img2yuv-webp")]
    WebP,
}
impl From<Compression> for image::ImageFormat {
    fn from(value: Compression) -> Self {
        match value {
            #[cfg(feature = "img2yuv-png")]
            Compression::Png => Self::Png,
            #[cfg(feature = "img2yuv-jpeg")]
            Compression::Jpeg => Self::Jpeg,
            #[cfg(feature = "img2yuv-webp")]
            Compression::WebP => Self::WebP,
        }
    }
}
impl FromStr for Compression {
    type Err = UnknownCompressionError;

    /// Parses a compressed image format string.
    ///
    /// This accepts a bare codec name (e.g. `"png"`), as used by the Foxglove `CompressedImage`
    /// schema, as well as a ROS `sensor_msgs/CompressedImage` format string, which additionally
    /// encodes the original pixel format and (for non-depth transports) a `compressed` marker:
    ///
    /// - `CODEC`
    /// - `ORIG_PIXFMT; CODEC compressed [COMPRESSED_PIXFMT]`
    ///
    /// The foxglove frontend interprets both of these the same way, so we do too.
    ///
    /// `ORIG_PIXFMT; compressedDepth CODEC` (the `compressed_depth_image_transport` format) is
    /// always reported as an unknown compression: the payload has a transport-specific header
    /// before the codec's data, so it can't be decoded as an ordinary compressed image of that
    /// codec.
    ///
    /// [ros1]: https://docs.ros.org/en/noetic/api/sensor_msgs/html/msg/CompressedImage.html
    /// [ros2]: https://docs.ros.org/en/rolling/p/sensor_msgs/msg/CompressedImage.html
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.trim().to_ascii_lowercase();
        if normalized.is_empty() || normalized.contains("compresseddepth") {
            return Err(UnknownCompressionError(s.to_string()));
        }

        let tokens: Vec<&str> = normalized
            .split(|c: char| c == ';' || c.is_whitespace())
            .filter(|token| !token.is_empty())
            .collect();

        #[cfg(feature = "img2yuv-jpeg")]
        if tokens.iter().any(|&t| t == "jpg" || t == "jpeg") {
            return Ok(Self::Jpeg);
        }
        #[cfg(feature = "img2yuv-png")]
        if tokens.contains(&"png") {
            return Ok(Self::Png);
        }
        #[cfg(feature = "img2yuv-webp")]
        if tokens.contains(&"webp") {
            return Ok(Self::WebP);
        }
        Err(UnknownCompressionError(s.to_string()))
    }
}
impl Compression {
    /// Returns the canonical format string for this compression.
    pub fn as_str(self) -> &'static str {
        match self {
            #[cfg(feature = "img2yuv-png")]
            Self::Png => "png",
            #[cfg(feature = "img2yuv-jpeg")]
            Self::Jpeg => "jpeg",
            #[cfg(feature = "img2yuv-webp")]
            Self::WebP => "webp",
        }
    }
}

/// A compressed image.
#[derive(Debug, Clone)]
pub struct CompressedImage<'a> {
    /// The compression format for this image.
    pub compression: Compression,
    /// The compressed image data.
    pub data: Cow<'a, [u8]>,
}
impl CompressedImage<'_> {
    /// Creates an owned compressed image, cloning if necessary.
    pub fn into_owned(self) -> CompressedImage<'static> {
        CompressedImage {
            compression: self.compression,
            data: self.data.into_owned().into(),
        }
    }

    /// Returns the image dimensions, as (width, height) in pixels.
    pub fn probe_dimensions(&self) -> Result<(u32, u32), Error> {
        let mut reader = ImageReader::new(Cursor::new(&self.data));
        reader.set_format(self.compression.into());
        reader.into_dimensions().map_err(Error::ReadDimensions)
    }

    /// Converts the compressed image to a YUV 4:2:0 image.
    pub fn to_yuv420<T: Yuv420Buffer>(&self, dst: &mut T) -> Result<(), Error> {
        let rgb = image::load_from_memory_with_format(&self.data, self.compression.into())
            .map_err(Error::Decompress)?
            .into_rgb8();
        let (width, height) = rgb.dimensions();
        let stride = width * 3;
        dst.validate_dimensions(width, height)?;
        rgb_to_yuv420(dst, RawImageEncoding::Rgb8, rgb.as_raw(), stride)
    }
}

#[cfg(test)]
mod tests {
    use super::Compression;

    fn check(input: &str, expect: Option<Compression>) {
        println!("{input:?} -> {expect:?}");
        let compression = input.parse::<Compression>().ok();
        assert_eq!(compression, expect);
    }

    #[test]
    #[cfg(feature = "img2yuv-jpeg")]
    fn test_from_str_jpeg() {
        check("jpeg", Some(Compression::Jpeg));
        check("bgr8; jpeg compressed bgr8", Some(Compression::Jpeg));
    }

    #[test]
    #[cfg(feature = "img2yuv-png")]
    fn test_from_str_png() {
        check("png", Some(Compression::Png));
        check("rgba8; png compressed", Some(Compression::Png));
    }

    #[test]
    #[cfg(feature = "img2yuv-webp")]
    fn test_from_str_webp() {
        check("webp", Some(Compression::WebP));
    }

    #[test]
    fn test_from_str_unknown() {
        check("gif", None);
        check("rgb8; gif compressed", None);
    }

    #[test]
    fn test_from_str_compressed_depth_is_unknown() {
        // `compressed_depth_image_transport` prepends a transport-specific header before the
        // codec's data, so we can't decode it as an ordinary compressed image, even though the
        // codec name (e.g. "png") is otherwise recognized.
        check("16UC1; compressedDepth png", None);
        check("32FC1; compressedDepth rvl", None);
    }
}
