//! A library for decoding images and converting them to YUV 4:2:0 planar.

#![warn(missing_docs)]

mod compressed;
mod message;
mod raw;
#[cfg(feature = "img2yuv-ros1")]
pub mod ros1;
#[cfg(feature = "img2yuv-ros2")]
pub mod ros2;
#[cfg(test)]
mod tests;

pub use self::compressed::{CompressedImage, Compression, UnknownCompressionError};
pub use self::message::ImageMessage;
pub use self::raw::{BayerCfa, Endian, RawImage, RawImageEncoding, UnknownEncodingError};

/// An error returned by this library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The image has either zero width, or zero height.
    #[error("image has either zero width, or zero height")]
    ZeroSized,
    /// The stride is less than the expected row size.
    #[error("stride {stride} for {which} is less than expected row size {row_size}")]
    StrideTooSmall {
        /// The name of the buffer.
        which: String,
        /// The stride size, in bytes.
        stride: u32,
        /// The row size, in bytes.
        row_size: usize,
    },
    /// The buffer size is less than the expected size in bytes.
    #[error("buffer size {actual} for {which} is less than expected size {expect}")]
    BufferTooSmall {
        /// The name of the buffer.
        which: String,
        /// The actual size of the buffer.
        actual: usize,
        /// The expected size of the buffer (`stride` * `height`).
        expect: usize,
    },
    /// The source and destination dimensions do not match.
    #[error("dimensions {actual:?} for {which} do not match expected dimensions {expect:?}")]
    DimensionMismatch {
        /// The name of the buffer.
        which: String,
        /// Source dimensions.
        expect: (u32, u32),
        /// Destination dimensions.
        actual: (u32, u32),
    },
    /// The dimensions are too big to be represented as a 32-bit integer.
    #[error("dimensions are too large for u32")]
    DimensionsTooLargeForU32,
    /// Both dimensions of the Bayer image must be even.
    #[error("bayer image dimensions must be even: {width}x{height}")]
    BayerDimensionsMustBeEven {
        /// The width of the image.
        width: u32,
        /// The height of the image.
        height: u32,
    },
    /// The width of the YUV 4:2:2 image must be even.
    #[error("yuv422 image width must be even: {width}")]
    Yuv422WidthMustBeEven {
        /// The width of the image.
        width: u32,
    },
    /// Failed to read the image dimensions.
    #[error("failed to read image dimensions: {0}")]
    ReadDimensions(#[source] image::ImageError),
    /// Failed to decompress the compressed image.
    #[error("failed to decompress image: {0}")]
    Decompress(#[source] image::ImageError),
    /// Failed to convert the image to YUV 4:2:0.
    #[error("failed to convert to yuv420: {0}")]
    ConvertToYuv420(#[source] yuv::YuvError),
}

/// A trait for a YUV 4:2:0 planar image buffer.
pub trait Yuv420Buffer {
    /// Returns the dimensions of the image as (width, height), in pixels.
    fn dimensions(&self) -> (u32, u32);

    /// Returns immutable slices into the y, u, and v planes.
    fn yuv(&self) -> (&[u8], &[u8], &[u8]);

    /// Returns mutable slices into the y, u, and v planes.
    fn yuv_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]);

    /// Returns the number of y, u, and v components in each row.
    fn yuv_strides(&self) -> (u32, u32, u32);

    /// Validates that the buffer dimensions match the specified width and height, and that the
    /// buffer is internally consistent.
    #[doc(hidden)]
    fn validate_dimensions(&self, width: u32, height: u32) -> Result<(), Error> {
        if self.dimensions() != (width, height) {
            return Err(Error::DimensionMismatch {
                which: "Yuv420Buffer".to_string(),
                expect: (width, height),
                actual: self.dimensions(),
            });
        }
        self.validate()
    }

    /// Validates that the buffer is internally consistent.
    #[doc(hidden)]
    fn validate(&self) -> Result<(), Error> {
        // Check that each plane's stride is large enough.
        let (width, height) = self.dimensions();
        let (y_stride, u_stride, v_stride) = self.yuv_strides();
        for (which, stride, row_size) in [
            ("y", y_stride, width as usize),
            ("u", u_stride, width as usize / 2),
            ("v", v_stride, width as usize / 2),
        ] {
            if (stride as usize) < row_size {
                return Err(Error::StrideTooSmall {
                    which: which.to_string(),
                    stride,
                    row_size,
                });
            }
        }

        // Check that each plane is large enough.
        let (y, u, v) = self.yuv();
        for (which, actual, expect) in [
            ("y", y.len(), y_stride as usize * height as usize),
            ("u", u.len(), u_stride as usize * height as usize / 2),
            ("v", v.len(), v_stride as usize * height as usize / 2),
        ] {
            if actual < expect {
                return Err(Error::BufferTooSmall {
                    which: which.to_string(),
                    actual,
                    expect,
                });
            }
        }

        Ok(())
    }
}

/// A YUV 4:2:0 planar image buffer, backed by a `Vec<u8>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Yuv420Vec {
    /// The YUV data.
    pub data: Vec<u8>,
    /// The width of the image.
    pub width: u32,
    /// The height of the image.
    pub height: u32,
}
impl Yuv420Vec {
    /// Allocates a new YUV 4:2:0 buffer for an image of the provided dimensions.
    ///
    /// # Panics
    ///
    /// Panics if width or height is not even. YUV 4:2:0 requires even dimensions
    /// for proper chroma subsampling.
    pub fn new(width: u32, height: u32) -> Self {
        assert!(
            width % 2 == 0 && height % 2 == 0,
            "YUV 4:2:0 requires even dimensions, got {width}x{height}"
        );
        let len = ((width as usize * height as usize) / 2) * 3;
        Self {
            data: vec![0; len],
            width,
            height,
        }
    }
}
impl Yuv420Buffer for Yuv420Vec {
    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn yuv(&self) -> (&[u8], &[u8], &[u8]) {
        let ylen = self.width as usize * self.height as usize;
        let ulen = ylen / 4;
        let (y, uv) = self.data.split_at(ylen);
        let (u, v) = uv.split_at(ulen);
        (y, u, v)
    }

    fn yuv_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
        let ylen = self.width as usize * self.height as usize;
        let ulen = ylen / 4;
        let (y, uv) = self.data.split_at_mut(ylen);
        let (u, v) = uv.split_at_mut(ulen);
        (y, u, v)
    }

    fn yuv_strides(&self) -> (u32, u32, u32) {
        (self.width, self.width / 2, self.width / 2)
    }
}

/// Either a compressed image, or a raw image.
#[derive(Debug, Clone)]
pub enum Image<'a> {
    /// A compressed image.
    Compressed(CompressedImage<'a>),
    /// A raw image.
    Raw(RawImage<'a>),
}
impl Image<'_> {
    /// Creates an owned image, cloning if necessary.
    pub fn into_owned(self) -> Image<'static> {
        match self {
            Image::Compressed(image) => Image::Compressed(image.into_owned()),
            Image::Raw(image) => Image::Raw(image.into_owned()),
        }
    }

    /// Returns the image dimensions, as (width, height) in pixels.
    ///
    /// This may fail for compressed images, if the image data cannot be parsed.
    pub fn probe_dimensions(&self) -> Result<(u32, u32), Error> {
        match self {
            Image::Compressed(image) => image.probe_dimensions(),
            Image::Raw(image) => Ok((image.width, image.height)),
        }
    }

    /// Converts the image to a YUV 4:2:0 image.
    pub fn to_yuv420<T: Yuv420Buffer>(&self, dst: &mut T) -> Result<(), Error> {
        match self {
            Image::Compressed(image) => image.to_yuv420(dst),
            Image::Raw(image) => image.to_yuv420(dst),
        }
    }
}
