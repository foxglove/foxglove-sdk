use std::borrow::Cow;

use super::{Error, Yuv420Buffer};

mod helpers;
#[cfg(test)]
mod tests;

pub(crate) use self::helpers::{mono_to_yuv420, rgb_to_yuv420, yuv422_to_yuv420};

/// Unknown raw image encoding.
#[derive(Debug, thiserror::Error)]
#[error("unknown raw image encoding: {0}")]
pub struct UnknownEncodingError(String);

/// Endianness for multi-byte numbers.
#[derive(Debug, Clone, Copy)]
pub enum Endian {
    /// Little-endian.
    Little,
    /// Big-endian.
    Big,
}

/// Supported encodings for raw images.
#[derive(Debug, Clone, Copy)]
pub enum RawImageEncoding {
    /// Pixel colors decomposed into R, G, and B channels.
    Rgb8,
    /// Pixel colors decomposed into R, G, B, and A channels.
    Rgba8,
    /// Pixel colors decomposed into B, G, and R channels.
    Bgr8,
    /// Pixel colors decomposed into B, G, R, and A channels.
    Bgra8,
    /// Pixel brightness represented as a unsigned 8-bit integer.
    Mono8,
    /// Pixel brightness represented as a unsigned 16-bit integer.
    Mono16(Endian),
    /// Pixel brightness represented as a 32-bit floating point value, from 0.0 to 1.0.
    Mono32F(Endian),
    /// Packed YUV 4:2:2 with channels ordered as U, Y1, V, Y2.
    Uyvy,
    /// Packed YUV 4:2:2 with channels ordered as Y1, U, Y2, V.
    Yuyv,
    /// A Bayer filter mosaic. Each 2x2 region in the image is encoded by a combination of R, B,
    /// and two G channels. The order in which the channels are laid out is specified by the inner
    /// [`BayerCfa`] variant.
    Bayer8(BayerCfa),
}
impl RawImageEncoding {
    /// Parses an encoding string, with a hint about endian-ness for multi-byte values.
    ///
    /// Note that endianness does not affect channel ordering.
    pub fn parse_endian(s: &str, endian: Endian) -> Result<Self, UnknownEncodingError> {
        match s {
            "rgb8" => Ok(Self::Rgb8),
            "rgba8" => Ok(Self::Rgba8),
            "bgr8" | "8UC3" => Ok(Self::Bgr8),
            "bgra8" => Ok(Self::Bgra8),
            "yuv422" | "uyvy" => Ok(Self::Uyvy),
            "yuv422_yuy2" | "yuyv" => Ok(Self::Yuyv),
            "mono8" | "8UC1" => Ok(Self::Mono8),
            "mono16" | "16UC1" => Ok(Self::Mono16(endian)),
            "32FC1" => Ok(Self::Mono32F(endian)),
            "bayer_bggr8" => Ok(Self::Bayer8(BayerCfa::Bggr)),
            "bayer_gbrg8" => Ok(Self::Bayer8(BayerCfa::Gbrg)),
            "bayer_grbg8" => Ok(Self::Bayer8(BayerCfa::Grbg)),
            "bayer_rggb8" => Ok(Self::Bayer8(BayerCfa::Rggb)),
            _ => Err(UnknownEncodingError(s.to_string())),
        }
    }

    /// Returns the number of bytes per rendered pixel for this encoding format.
    ///
    /// In other words, this value can be multiplied by an image's width to derive the number of
    /// bytes per row. This interpretation is helpful when considering subsampled formats like
    /// bayer and packed YUV 4:2:2.
    pub(crate) fn bytes_per_pixel(self) -> u8 {
        match self {
            Self::Mono8 | Self::Bayer8(_) => 1,
            Self::Yuyv | Self::Uyvy | Self::Mono16(_) => 2,
            Self::Rgb8 | Self::Bgr8 => 3,
            Self::Rgba8 | Self::Bgra8 | Self::Mono32F(_) => 4,
        }
    }

    /// Returns true if this is a subsampled format.
    fn is_subsampled(self) -> bool {
        matches!(self, Self::Bayer8(_) | Self::Uyvy | Self::Yuyv)
    }
}

/// Bayer color filter array format.
#[derive(Debug, Clone, Copy)]
pub enum BayerCfa {
    /// ```text
    /// B | G
    /// -----
    /// G | R
    /// ```
    Bggr,
    /// ```text
    /// G | B
    /// -----
    /// R | G
    /// ```
    Gbrg,
    /// ```text
    /// G | R
    /// -----
    /// B | G
    /// ```
    Grbg,
    /// ```text
    /// R | G
    /// -----
    /// G | B
    /// ```
    Rggb,
}

/// A raw, uncompressed image.
#[derive(Debug, Clone)]
pub struct RawImage<'a> {
    /// The image encoding.
    pub encoding: RawImageEncoding,
    /// The width of the image, in pixels.
    ///
    /// For Bayer images, this is the number of subpixels, which is always an even number.
    pub width: u32,
    /// The height of the image, in pixels.
    ///
    /// For Bayer images, this is the number of subpixels, which is always an even number.
    pub height: u32,
    /// The stride of an image row, in bytes. This value must be large enough to hold
    /// an entire row of pixels. In other words, if a pixel is N bytes, the stride must
    /// be greater than or equal to N * width. For Bayer images, N is considered to be 1.
    pub stride: u32,
    /// The image data.
    pub data: Cow<'a, [u8]>,
}
impl RawImage<'_> {
    /// Creates an owned compressed image, cloning if necessary.
    pub fn into_owned(self) -> RawImage<'static> {
        RawImage {
            encoding: self.encoding,
            width: self.width,
            height: self.height,
            stride: self.stride,
            data: self.data.into_owned().into(),
        }
    }

    /// Converts the raw image to a YUV 4:2:0 image.
    pub fn to_yuv420<T: Yuv420Buffer>(&self, dst: &mut T) -> Result<(), Error> {
        self.validate_dimensions()?;
        dst.validate_dimensions(self.width, self.height)?;
        match self.encoding {
            // RGB formats
            RawImageEncoding::Rgb8
            | RawImageEncoding::Rgba8
            | RawImageEncoding::Bgr8
            | RawImageEncoding::Bgra8 => rgb_to_yuv420(dst, self.encoding, &self.data, self.stride),

            // Packed YUV 4:2:2 formats
            RawImageEncoding::Uyvy | RawImageEncoding::Yuyv => {
                yuv422_to_yuv420(dst, self.encoding, &self.data, self.stride)
            }

            // For mono formats, convert to [0.0, 1.0], then rescale into the limited Y range.
            RawImageEncoding::Mono8 => {
                mono_to_yuv420(dst, self.pixels::<1>().map(|p| f32::from(p[0]) / 255.0));
                Ok(())
            }
            RawImageEncoding::Mono16(endian) => {
                let read_u16 = if matches!(endian, Endian::Big) {
                    u16::from_be_bytes
                } else {
                    u16::from_le_bytes
                };
                mono_to_yuv420(
                    dst,
                    self.pixels::<2>()
                        .copied()
                        .map(|p| f32::from(read_u16(p)) / 65535.0),
                );
                Ok(())
            }
            RawImageEncoding::Mono32F(endian) => {
                let read_f32 = if matches!(endian, Endian::Big) {
                    f32::from_be_bytes
                } else {
                    f32::from_le_bytes
                };
                mono_to_yuv420(dst, self.pixels::<4>().copied().map(read_f32));
                Ok(())
            }

            // For Bayer images, encode to RGB first, then YUV 4:2:0.
            //
            // Use the same trivial algorithm as the foxglove viz. For each 2x2 region, copy R &
            // B values to all four pixels. Use the first row's G value for the first row, and the
            // second row's G value for the second row.
            //
            // We might consider using a linear/cubic interpolation from the `bayer` crate, as an
            // alternative.
            RawImageEncoding::Bayer8(_) => {
                let stride = self
                    .width
                    .checked_mul(3)
                    .ok_or(Error::DimensionsTooLargeForU32)?;
                let width = self.width as usize;
                let rgb_size = (stride as usize)
                    .checked_mul(self.height as usize)
                    .ok_or(Error::DimensionsTooLargeForU32)?;
                let mut data = vec![0; rgb_size];
                let pixels = self.bayer8_pixels().expect("dimensions already validated");
                for bp @ BayerPixel { r, g0, g1, b, .. } in pixels {
                    // Populate 2x2 RGB8 pixels.
                    let row = bp.row as usize;
                    let col = bp.col as usize;
                    let r0 = 3 * (row * width + col);
                    let r1 = 3 * ((row + 1) * width + col);
                    data[r0..r0 + 6].copy_from_slice(&[r, g0, b, r, g0, b]);
                    data[r1..r1 + 6].copy_from_slice(&[r, g1, b, r, g1, b]);
                }
                rgb_to_yuv420(dst, RawImageEncoding::Rgb8, &data, stride)
            }
        }
    }

    /// Validates the image dimensions.
    fn validate_dimensions(&self) -> Result<(), Error> {
        // Ensure that the image has non-zero width and height.
        if self.width == 0 || self.height == 0 {
            return Err(Error::ZeroSized);
        }

        // Ensure that the stride includes an entire row, plus optional padding.
        let row_size = self.width as usize * self.encoding.bytes_per_pixel() as usize;
        if (self.stride as usize) < row_size {
            return Err(Error::StrideTooSmall {
                which: "RawImage".to_string(),
                stride: self.stride,
                row_size,
            });
        }

        // Ensure that the buffer is large enough for the specified dimensions.
        let expect = self.height as usize * self.stride as usize;
        if self.data.len() < expect {
            return Err(Error::BufferTooSmall {
                which: "RawImage".to_string(),
                actual: self.data.len(),
                expect,
            });
        }

        // For Bayer and YUV 4:2:2 formats, some image dimensions must be even. We could handle odd
        // dimensions by synthesizing subpixels, but it's probably not worth the effort.
        match (self.encoding, self.width % 2 == 0, self.height % 2 == 0) {
            (RawImageEncoding::Bayer8(_), w, h) if !(w && h) => {
                return Err(Error::BayerDimensionsMustBeEven {
                    width: self.width,
                    height: self.height,
                });
            }
            (RawImageEncoding::Uyvy | RawImageEncoding::Yuyv, false, _) => {
                return Err(Error::Yuv422WidthMustBeEven { width: self.width });
            }
            _ => (),
        }

        Ok(())
    }
}

impl<'a> RawImage<'a> {
    /// Returns an iterator over pixels in an image.
    ///
    /// Panics if this is a subsampled encoding, or if N is not equal to the expected number of
    /// bytes per pixel for this encoding.
    fn pixels<const N: usize>(&'a self) -> Pixels<'a, N> {
        debug_assert!(!self.encoding.is_subsampled());
        debug_assert_eq!(self.encoding.bytes_per_pixel() as usize, N);
        Pixels::new(self)
    }

    /// Returns an iterator over pixels in a Bayer image.
    ///
    /// Panics if this is not a Bayer image.
    fn bayer8_pixels(&'a self) -> Result<Bayer8Pixels<'a>, Error> {
        debug_assert!(matches!(self.encoding, RawImageEncoding::Bayer8(_)));
        let RawImageEncoding::Bayer8(cfa) = self.encoding else {
            unreachable!();
        };
        Bayer8Pixels::new(self, cfa)
    }
}

/// A pixel reader, which reads an array of subpixels at a time.
struct Pixels<'a, const N: usize> {
    data: &'a [u8],
    width: usize,
    height: usize,
    stride: usize,
    row: usize,
    col: usize,
}
impl<'a, const N: usize> Pixels<'a, N> {
    fn new(image: &'a RawImage<'a>) -> Self {
        Self {
            data: &image.data,
            width: image.width as usize,
            height: image.height as usize,
            stride: image.stride as usize,
            row: 0,
            col: 0,
        }
    }
}
impl<'a, const N: usize> Iterator for Pixels<'a, N> {
    type Item = &'a [u8; N];

    fn next(&mut self) -> Option<Self::Item> {
        if self.row == self.height {
            return None;
        }
        let pos = self.row * self.stride + self.col * N;
        if self.col == self.width - 1 {
            self.col = 0;
            self.row += 1;
        } else {
            self.col += 1;
        }
        Some(self.data[pos..pos + N].try_into().unwrap())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.height - self.row) * self.width - self.col;
        (remaining, Some(remaining))
    }
}
impl<const N: usize> ExactSizeIterator for Pixels<'_, N> {}

/// A Bayer 2x2 composite pixel.
#[derive(Debug, Clone, PartialEq, Eq)]
struct BayerPixel {
    /// The red value.
    r: u8,
    /// The green value from the upper row.
    g0: u8,
    /// The green value from the lower row.
    g1: u8,
    /// The blue value.
    b: u8,
    /// The row of the upper-left pixel. Always an even number.
    row: u32,
    /// The column of the upper-left pixel. Always an even number.
    col: u32,
}
struct Bayer8Pixels<'a> {
    data: &'a [u8],
    cfa: BayerCfa,
    width: usize,
    height: usize,
    stride: usize,
    row: usize,
    col: usize,
}
impl<'a> Bayer8Pixels<'a> {
    fn new(image: &'a RawImage<'a>, cfa: BayerCfa) -> Result<Self, Error> {
        // For now, we choose not to handle odd-dimensioned images. If we want to do so in the
        // future, we could synthesize remaining subpixel values from neighbors.
        let (width, height) = (image.width, image.height);
        if width % 2 != 0 || height % 2 != 0 {
            return Err(Error::BayerDimensionsMustBeEven { width, height });
        }
        Ok(Self {
            data: &image.data,
            cfa,
            width: image.width as usize,
            height: image.height as usize,
            stride: image.stride as usize,
            row: 0,
            col: 0,
        })
    }
}
impl<'a> Iterator for Bayer8Pixels<'a> {
    type Item = BayerPixel;

    fn next(&mut self) -> Option<Self::Item> {
        if self.row == self.height {
            return None;
        }
        let r0 = self.row * self.stride + self.col;
        let r1 = r0 + self.stride;
        let v = (
            self.data[r0],
            self.data[r0 + 1],
            self.data[r1],
            self.data[r1 + 1],
        );
        let (r, g0, g1, b) = match self.cfa {
            BayerCfa::Bggr => (v.3, v.1, v.2, v.0),
            BayerCfa::Gbrg => (v.2, v.0, v.3, v.1),
            BayerCfa::Grbg => (v.1, v.0, v.3, v.2),
            BayerCfa::Rggb => v,
        };
        let pixel = BayerPixel {
            r,
            g0,
            g1,
            b,
            row: self.row as u32,
            col: self.col as u32,
        };
        if self.col == self.width - 2 {
            self.col = 0;
            self.row += 2;
        } else {
            self.col += 2;
        }
        Some(pixel)
    }
}
