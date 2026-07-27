use std::{borrow::Cow, io::Cursor};

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
impl Compression {
    /// Parses a bare codec name (e.g. `"png"`).
    ///
    /// To parse a ROS-style format string, use [`Compression::try_from_ros_format`] instead.
    fn from_codec(codec: &str) -> Option<Self> {
        match codec {
            #[cfg(feature = "img2yuv-png")]
            "png" => Some(Self::Png),
            #[cfg(feature = "img2yuv-jpeg")]
            "jpg" | "jpeg" => Some(Self::Jpeg),
            #[cfg(feature = "img2yuv-webp")]
            "webp" => Some(Self::WebP),
            _ => None,
        }
    }

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

    /// Parses a format string from a ROS `CompressedImage` message.
    ///
    /// This accepts a bare codec name (e.g. `"png"`), as well as the documented [ROS 1][ros1] and
    /// [ROS 2][ros2] format string, which additionally encodes the original pixel format and a
    /// `compressed` marker:
    ///
    /// - `CODEC`
    /// - `ORIG_PIXFMT; CODEC compressed [COMPRESSED_PIXFMT]`
    ///
    /// Other strings are rejected even if they contain a codec token.
    ///
    /// Note that `ORIG_PIXFMT; compressedDepth CODEC` (the `compressed_depth_image_transport`
    /// format) is always reported as an unknown compression: the payload has a transport-specific
    /// header before the codec's data, so it can't be decoded as an ordinary compressed image of
    /// that codec.
    ///
    /// [ros1]: https://docs.ros.org/en/noetic/api/sensor_msgs/html/msg/CompressedImage.html
    /// [ros2]: https://docs.ros.org/en/rolling/p/sensor_msgs/msg/CompressedImage.html
    pub fn try_from_ros_format(format: &str) -> Result<Self, UnknownCompressionError> {
        let normalized = format.trim().to_ascii_lowercase();
        let unknown = || UnknownCompressionError(format.to_string());

        let Some((orig, rest)) = normalized.split_once(';') else {
            return Self::from_codec(&normalized).ok_or_else(unknown);
        };
        let orig: Vec<&str> = orig.split_whitespace().collect();
        let rest: Vec<&str> = rest.split_whitespace().collect();

        match (orig.as_slice(), rest.as_slice()) {
            ([_], [codec, "compressed"] | [codec, "compressed", _]) => {
                Self::from_codec(codec).ok_or_else(unknown)
            }
            _ => Err(unknown()),
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

    fn check_ros_format(input: &str, expect: Option<Compression>) {
        println!("{input:?} -> {expect:?}");
        let compression = Compression::try_from_ros_format(input).ok();
        assert_eq!(compression, expect);
    }

    #[test]
    #[cfg(feature = "img2yuv-jpeg")]
    fn test_try_from_ros_format_jpeg() {
        check_ros_format("jpeg", Some(Compression::Jpeg));
        check_ros_format("  JPG  ", Some(Compression::Jpeg));
        check_ros_format("bgr8; jpeg compressed bgr8", Some(Compression::Jpeg));
        check_ros_format("BGR8; JPEG compressed RGB8", Some(Compression::Jpeg));
    }

    #[test]
    #[cfg(feature = "img2yuv-png")]
    fn test_try_from_ros_format_png() {
        check_ros_format("png", Some(Compression::Png));
        check_ros_format("rgba8; png compressed", Some(Compression::Png));
    }

    #[test]
    #[cfg(feature = "img2yuv-webp")]
    fn test_try_from_ros_format_webp() {
        check_ros_format("webp", Some(Compression::WebP));
    }

    #[test]
    fn test_try_from_ros_format_unknown() {
        check_ros_format("gif", None);
        check_ros_format("rgb8; gif compressed", None);
        check_ros_format("rgb8; compressed jpeg", None);
        check_ros_format("some unrelated png metadata", None);
        check_ros_format("png jpeg", None);
        check_ros_format("rgb8; png", None);
        check_ros_format("rgb8; png compressed bgr8 extra", None);
    }

    #[test]
    fn test_try_from_ros_format_compressed_depth_is_unknown() {
        // `compressed_depth_image_transport` prepends a transport-specific header before the
        // codec's data, so we can't decode it as an ordinary compressed image, even though the
        // codec name (e.g. "png") is otherwise recognized.
        check_ros_format("16UC1; compressedDepth png", None);
        check_ros_format("32FC1; compressedDepth rvl", None);
    }
}
