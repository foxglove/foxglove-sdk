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

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            #[cfg(feature = "img2yuv-png")]
            "png" => Ok(Self::Png),
            #[cfg(feature = "img2yuv-jpeg")]
            "jpg" | "jpeg" => Ok(Self::Jpeg),
            #[cfg(feature = "img2yuv-webp")]
            "webp" => Ok(Self::WebP),
            _ => Err(UnknownCompressionError(s.to_string())),
        }
    }
}
impl Compression {
    /// Parses a format string from a ROS `CompressedImage` message.
    ///
    /// The format string is described at length in the ROS 2 [sensor_msgs/CompressedImage][ros2]
    /// definition.
    ///
    /// [ros1]: https://docs.ros.org/en/noetic/api/sensor_msgs/html/msg/CompressedImage.html
    /// [ros2]: https://docs.ros.org/en/rolling/p/sensor_msgs/msg/CompressedImage.html
    #[cfg(any(feature = "img2yuv-ros1", feature = "img2yuv-ros2"))]
    pub fn try_from_ros_format(format: &str) -> Result<Self, UnknownCompressionError> {
        use regex::Regex;
        use std::sync::LazyLock;

        // ORIG_PIXFMT; CODEC compressed [COMPRESSED_PIXFMT]
        static COMPRESSED: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^\S+; (?<codec>\S+) compressed( \S+)?$").unwrap());

        // ORIG_PIXFMT; compressedDepth CODEC
        static COMPRESSED_DEPTH: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^\S+; compressedDepth (?<codec>\S+)$").unwrap());

        let mut codec = format;
        for pattern in [&COMPRESSED, &COMPRESSED_DEPTH] {
            if let Some(cap) = pattern.captures(format) {
                codec = cap.name("codec").unwrap().as_str();
                break;
            }
        }
        codec.parse()
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
#[cfg(any(feature = "img2yuv-ros1", feature = "img2yuv-ros2"))]
mod tests {
    use super::Compression;

    fn check(input: &str, expect: Option<Compression>) {
        println!("{input:?} -> {expect:?}");
        let compression = Compression::try_from_ros_format(input).ok();
        assert_eq!(compression, expect);
    }

    #[test]
    #[cfg(feature = "img2yuv-jpeg")]
    fn test_try_from_ros_format_jpeg() {
        check("jpeg", Some(Compression::Jpeg));
        check("bgr8; jpeg compressed bgr8", Some(Compression::Jpeg));
    }

    #[test]
    #[cfg(feature = "img2yuv-png")]
    fn test_try_from_ros_format_png() {
        check("png", Some(Compression::Png));
        check("rgba8; png compressed", Some(Compression::Png));
        check("16UC1; compressedDepth png", Some(Compression::Png));
    }

    #[test]
    #[cfg(feature = "img2yuv-webp")]
    fn test_try_from_ros_format_webp() {
        check("webp", Some(Compression::WebP));
    }

    #[test]
    fn test_try_from_ros_format_unknown() {
        check("gif", None);
        check("rgb8; gif compressed", None);
        check("32FC1; compressedDepth rvl", None);
    }
}
