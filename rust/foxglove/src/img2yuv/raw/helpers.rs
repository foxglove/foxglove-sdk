use std::borrow::Cow;

use yuv::{
    BufferStoreMut, YuvConversionMode, YuvPackedImage, YuvPlanarImageMut, YuvRange,
    YuvStandardMatrix,
};

use super::{Error, RawImageEncoding, Yuv420Buffer};

/// Returns a YuvPlanarImageMut wrapper for the buffer.
fn wrapped(buf: &mut impl Yuv420Buffer) -> YuvPlanarImageMut<'_, u8> {
    let (width, height) = buf.dimensions();
    let (y_stride, u_stride, v_stride) = buf.yuv_strides();
    let (y, u, v) = buf.yuv_mut();
    YuvPlanarImageMut {
        y_plane: BufferStoreMut::Borrowed(y),
        y_stride,
        u_plane: BufferStoreMut::Borrowed(u),
        u_stride,
        v_plane: BufferStoreMut::Borrowed(v),
        v_stride,
        width,
        height,
    }
}

// These are intended to be conservative defaults that will work well enough, even if (for example)
// we can't provide VUI parameters in the H.264 stream. In the absence of VUI, an H.264 decoder is
// likely to use BT.601 or BT.709 (based on resolution), and limited range. We might consider
// exposing API controls for customizing these parameters on a case-by-case basis.
const RANGE: YuvRange = YuvRange::Limited;
const MATRIX: YuvStandardMatrix = YuvStandardMatrix::Bt709;
const MODE: YuvConversionMode = YuvConversionMode::Balanced;

/// Converts a raw RGB image to a YUV 4:2:0 planar image.
pub(crate) fn rgb_to_yuv420<T: Yuv420Buffer>(
    dst: &mut T,
    encoding: RawImageEncoding,
    data: &[u8],
    stride: u32,
) -> Result<(), Error> {
    let conv = match encoding {
        RawImageEncoding::Rgb8 => yuv::rgb_to_yuv420,
        RawImageEncoding::Rgba8 => yuv::rgba_to_yuv420,
        RawImageEncoding::Bgr8 => yuv::bgr_to_yuv420,
        RawImageEncoding::Bgra8 => yuv::bgra_to_yuv420,
        _ => unreachable!(),
    };
    conv(&mut wrapped(dst), data, stride, RANGE, MATRIX, MODE).map_err(Error::ConvertToYuv420)
}

/// Converts a packed YUV 4:2:2 image to a YUV 4:2:0 planar image.
pub(crate) fn yuv422_to_yuv420<T: Yuv420Buffer>(
    dst: &mut T,
    encoding: RawImageEncoding,
    data: &[u8],
    stride: u32,
) -> Result<(), Error> {
    // For now, we choose not to handle odd-width images. If we want to do so in the future, we
    // could duplicate the last (Y, U) values in each row.
    let (width, height) = dst.dimensions();
    if width % 2 != 0 {
        return Err(Error::Yuv422WidthMustBeEven { width });
    }

    // The yuv crate expects the stride to be exactly 2 * width for even-width images,
    // or 2 * (width + 1) for odd-width images.
    let expected_stride = 2 * width;
    let yuy = if stride == expected_stride {
        Cow::Borrowed(data)
    } else {
        let h = height as usize;
        let s = stride as usize;
        let row_len = 2 * width as usize;
        let mut buf: Vec<u8> = Vec::with_capacity(h * row_len);
        for row in 0..h {
            let pos = row * s;
            buf.extend(&data[pos..pos + row_len]);
        }
        Cow::Owned(buf)
    };

    let conv = match encoding {
        RawImageEncoding::Yuyv => yuv::yuyv422_to_yuv420,
        RawImageEncoding::Uyvy => yuv::uyvy422_to_yuv420,
        _ => unreachable!(),
    };
    let src = YuvPackedImage {
        yuy: yuy.as_ref(),
        yuy_stride: width * 2,
        width,
        height,
    };
    conv(&mut wrapped(dst), &src).map_err(Error::ConvertToYuv420)
}

/// Converts a mono image, represented as floating point luma values on the range [0.0, 1.0], to a
/// YUV 4:2:0 planar image.
///
/// The caller must ensure that the iterator yields exactly `width * height` items.
pub(crate) fn mono_to_yuv420<T: Yuv420Buffer>(dst: &mut T, data: impl IntoIterator<Item = f32>) {
    // The luma rescaling below assumes limited range.
    const { assert!(matches!(RANGE, YuvRange::Limited)) }

    let (width, height) = dst.dimensions();
    let (y_stride, _, _) = dst.yuv_strides();
    let (y, u, v) = dst.yuv_mut();
    let mut count = 0;
    for (i, p) in data.into_iter().enumerate() {
        let row = i / width as usize;
        let col = i % width as usize;
        let pos = col + row * y_stride as usize;
        y[pos] = (16.0 + p.clamp(0.0, 1.0) * 219.0).round() as u8;
        count = i + 1;
    }
    debug_assert_eq!(
        count,
        width as usize * height as usize,
        "iterator yielded wrong number of pixels"
    );
    u.fill(128);
    v.fill(128);
}
