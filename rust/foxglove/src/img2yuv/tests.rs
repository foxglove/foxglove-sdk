use std::{collections::HashMap, path::PathBuf, sync::LazyLock};

use assert_matches::assert_matches;

use super::{BayerCfa, Endian, Error, Image, RawImage, RawImageEncoding, Yuv420Buffer, Yuv420Vec};
#[cfg(any(
    feature = "img2yuv-jpeg",
    feature = "img2yuv-png",
    feature = "img2yuv-webp"
))]
use super::{CompressedImage, Compression};

/// These constants should be kept in sync with testdata/gen/main.py.
const W: u32 = 64;
const H: u32 = 16;
const PAD: u32 = 5;

fn testdata_path(name: &str) -> PathBuf {
    let mut abs = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    abs.push("src");
    abs.push("img2yuv");
    abs.push("testdata");
    abs.push(name);
    abs
}

fn load_testdata(name: &str) -> std::io::Result<Vec<u8>> {
    std::fs::read(testdata_path(name))
}

/// The type of reference image to compare against.
///
/// Note that YUV420 does not have an alpha channel. The transcoder handles this by discarding the
/// alpha channel, so we don't have separate reference for source formats that include an alpha
/// channel.
#[derive(Debug, Clone, Copy)]
enum Reference {
    Rgb,
    Mono,
}
impl Reference {
    fn from_file_name(name: &str) -> Self {
        if name.contains("mono") {
            Self::Mono
        } else {
            Self::Rgb
        }
    }
}

#[derive(Debug)]
struct TestImage {
    name: String,
    image: Image<'static>,
    reference: Reference,
}
impl TestImage {
    fn raw(name: &str, encoding: RawImageEncoding) -> std::io::Result<Self> {
        load_testdata(name).map(|data| Self {
            name: name.to_string(),
            reference: Reference::from_file_name(name),
            image: Image::Raw(RawImage {
                encoding,
                width: W,
                height: H,
                stride: W * encoding.bytes_per_pixel() as u32,
                data: data.into(),
            }),
        })
    }

    fn raw_pad(name: &str, encoding: RawImageEncoding) -> std::io::Result<Self> {
        load_testdata(name).map(|data| Self {
            name: name.to_string(),
            reference: Reference::from_file_name(name),
            image: Image::Raw(RawImage {
                encoding,
                width: W,
                height: H,
                stride: PAD + W * encoding.bytes_per_pixel() as u32,
                data: data.into(),
            }),
        })
    }

    #[cfg(any(
        feature = "img2yuv-jpeg",
        feature = "img2yuv-png",
        feature = "img2yuv-webp"
    ))]
    fn compressed(name: &str, compression: Compression) -> std::io::Result<Self> {
        load_testdata(name).map(|data| Self {
            name: name.to_string(),
            reference: Reference::from_file_name(name),
            image: Image::Compressed(CompressedImage {
                compression,
                data: data.into(),
            }),
        })
    }

    fn to_yuv420(&self) -> Result<Yuv420Vec, super::Error> {
        let (width, height) = self.image.probe_dimensions()?;
        let mut buf = Yuv420Vec::new(width, height);
        self.image.to_yuv420(&mut buf)?;
        Ok(buf)
    }
}

fn load_test_images() -> std::io::Result<Vec<TestImage>> {
    Ok(vec![
        TestImage::raw_pad(
            "test.bayer8-bggr.pad.raw",
            RawImageEncoding::Bayer8(BayerCfa::Bggr),
        )?,
        TestImage::raw(
            "test.bayer8-bggr.raw",
            RawImageEncoding::Bayer8(BayerCfa::Bggr),
        )?,
        TestImage::raw_pad(
            "test.bayer8-gbrg.pad.raw",
            RawImageEncoding::Bayer8(BayerCfa::Gbrg),
        )?,
        TestImage::raw(
            "test.bayer8-gbrg.raw",
            RawImageEncoding::Bayer8(BayerCfa::Gbrg),
        )?,
        TestImage::raw_pad(
            "test.bayer8-grbg.pad.raw",
            RawImageEncoding::Bayer8(BayerCfa::Grbg),
        )?,
        TestImage::raw(
            "test.bayer8-grbg.raw",
            RawImageEncoding::Bayer8(BayerCfa::Grbg),
        )?,
        TestImage::raw_pad(
            "test.bayer8-rggb.pad.raw",
            RawImageEncoding::Bayer8(BayerCfa::Rggb),
        )?,
        TestImage::raw(
            "test.bayer8-rggb.raw",
            RawImageEncoding::Bayer8(BayerCfa::Rggb),
        )?,
        TestImage::raw("test.bgr8.raw", RawImageEncoding::Bgr8)?,
        TestImage::raw_pad("test.bgr8.pad.raw", RawImageEncoding::Bgr8)?,
        TestImage::raw("test.bgra8.raw", RawImageEncoding::Bgra8)?,
        TestImage::raw_pad("test.bgra8.pad.raw", RawImageEncoding::Bgra8)?,
        #[cfg(feature = "img2yuv-jpeg")]
        TestImage::compressed("test.jpg", Compression::Jpeg)?,
        TestImage::raw_pad(
            "test.mono16be.pad.raw",
            RawImageEncoding::Mono16(Endian::Big),
        )?,
        TestImage::raw("test.mono16be.raw", RawImageEncoding::Mono16(Endian::Big))?,
        TestImage::raw_pad(
            "test.mono16le.pad.raw",
            RawImageEncoding::Mono16(Endian::Little),
        )?,
        TestImage::raw(
            "test.mono16le.raw",
            RawImageEncoding::Mono16(Endian::Little),
        )?,
        TestImage::raw_pad(
            "test.mono32fbe.pad.raw",
            RawImageEncoding::Mono32F(Endian::Big),
        )?,
        TestImage::raw("test.mono32fbe.raw", RawImageEncoding::Mono32F(Endian::Big))?,
        TestImage::raw_pad(
            "test.mono32fle.pad.raw",
            RawImageEncoding::Mono32F(Endian::Little),
        )?,
        TestImage::raw(
            "test.mono32fle.raw",
            RawImageEncoding::Mono32F(Endian::Little),
        )?,
        TestImage::raw_pad("test.mono8.pad.raw", RawImageEncoding::Mono8)?,
        TestImage::raw("test.mono8.raw", RawImageEncoding::Mono8)?,
        #[cfg(feature = "img2yuv-png")]
        TestImage::compressed("test.png", Compression::Png)?,
        TestImage::raw_pad("test.rgb8.pad.raw", RawImageEncoding::Rgb8)?,
        TestImage::raw("test.rgb8.raw", RawImageEncoding::Rgb8)?,
        TestImage::raw_pad("test.rgba8.pad.raw", RawImageEncoding::Rgba8)?,
        TestImage::raw("test.rgba8.raw", RawImageEncoding::Rgba8)?,
        TestImage::raw_pad("test.uyvy.pad.raw", RawImageEncoding::Uyvy)?,
        TestImage::raw("test.uyvy.raw", RawImageEncoding::Uyvy)?,
        TestImage::raw_pad("test.yuyv.pad.raw", RawImageEncoding::Yuyv)?,
        TestImage::raw("test.yuyv.raw", RawImageEncoding::Yuyv)?,
        #[cfg(feature = "img2yuv-webp")]
        TestImage::compressed("test.webp", Compression::WebP)?,
    ])
}

static TEST_IMAGES: LazyLock<HashMap<String, TestImage>> = LazyLock::new(|| {
    load_test_images()
        .expect("failed to load test images")
        .into_iter()
        .map(|ti| (ti.name.clone(), ti))
        .collect()
});

fn get_image(file: &str) -> &'static TestImage {
    TEST_IMAGES.get(file).expect("test image not loaded")
}

fn get_reference(reference: Reference) -> &'static TestImage {
    let name = match reference {
        Reference::Rgb => "test.rgb8.raw",
        Reference::Mono => "test.mono32fle.raw",
    };
    TEST_IMAGES.get(name).expect("reference image not loaded")
}

fn test_reference_snapshot(reference: Reference, name: &str) {
    let yuv420 = get_reference(reference).to_yuv420().unwrap();
    assert_eq!(yuv420.height, H);
    assert_eq!(yuv420.width, W);
    insta::assert_binary_snapshot!(name, yuv420.data);
}

// The `yuv` crate uses SIMD, and ends up with slightly different results for RGB images, depending
// on the instruction set (AVX2, SSE4.1, NEON, RDM, fallback). The differences are limited to
// off-by-one errors, probably due to rounding. That's not something we actually care about, but
// it's enough to trip up snapshot tests.
//
// AVX2 and SSE4.1 produce equivalent results for our test data, so only enable this test when one
// or the other is available.
#[test]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn test_rgb_reference_snapshot() {
    let avx = std::arch::is_x86_feature_detected!("avx2");
    let sse = std::arch::is_x86_feature_detected!("sse4.1");
    if avx || sse {
        test_reference_snapshot(Reference::Rgb, "rgb.yuv420.raw");
    }
}

#[test]
fn test_mono_reference_snapshot() {
    test_reference_snapshot(Reference::Mono, "mono.yuv420.raw");
}

#[allow(dead_code)]
#[derive(Debug)]
struct Diff {
    mean_sq: f64,
    mean_abs: f64,
    max_abs: u8,
    psnr: f64,
}
fn diff(a: &[u8], b: &[u8]) -> Diff {
    assert_eq!(a.len(), b.len());

    // Compute mean squared error, and some other stats for debug logging.
    let mut sum_sq: u64 = 0;
    let mut sum_abs: u64 = 0;
    let mut max_abs: u8 = 0;
    for d in a.iter().zip(b).map(|(&av, &bv)| av as i16 - bv as i16) {
        let abs = d.unsigned_abs();
        sum_abs += abs as u64;
        sum_sq += (abs as u64).pow(2);
        if abs > max_abs as u16 {
            max_abs = abs as u8
        };
    }
    let n = a.len() as f64;
    let mean_sq = sum_sq as f64 / n;
    let mean_abs = sum_abs as f64 / n;

    // Derive peak signal-to-noise ratio from mean squared error.
    // https://en.wikipedia.org/wiki/Peak_signal-to-noise_ratio
    let peak = 255.0;
    let psnr = {
        if mean_sq == 0.0 {
            f64::INFINITY
        } else {
            10.0 * ((peak * peak) / mean_sq).log10()
        }
    };

    Diff {
        mean_sq,
        mean_abs,
        max_abs,
        psnr,
    }
}

#[derive(Debug)]
struct Yuv420Diff {
    y: Diff,
    uv: Diff,
}
fn diff_yuv420(a: &Yuv420Vec, b: &Yuv420Vec) -> Yuv420Diff {
    assert_eq!(a.width, b.width);
    assert_eq!(a.height, b.height);
    let ylen = a.width as usize * a.height as usize;
    let y = diff(&a.data[0..ylen], &b.data[0..ylen]);
    let uv = diff(&a.data[ylen..], &b.data[ylen..]);
    Yuv420Diff { y, uv }
}

const ORANGE: &str = "\x1b[38;2;255;165;0m";
const RESET: &str = "\x1b[0m";

fn print_hexdump_diff(prefix: &str, a_label: &str, a: &[u8], b_label: &str, b: &[u8]) {
    println!("{prefix}{a_label: <63}     {b_label}");
    for (a_line, b_line) in hexdump::hexdump_iter(a).zip(hexdump::hexdump_iter(b)) {
        let a_str = &*a_line;
        let b_str = &*b_line;
        assert_eq!(a_str.len(), b_str.len());
        let mut b_hilite = String::with_capacity(a_str.len());
        for (a_chr, b_chr) in a_str.chars().zip(b_str.chars()) {
            if a_chr == b_chr {
                b_hilite.push(b_chr);
            } else {
                b_hilite.push_str(ORANGE);
                b_hilite.push(b_chr);
                b_hilite.push_str(RESET);
            }
        }
        let sym = if *a_line == *b_line { " " } else { "!" };
        println!("{prefix}{a_line: <63}  {sym}  {b_hilite}");
    }
}

fn test_against_reference(file: &str, y_psnr: f64, uv_psnr: f64) {
    let img = get_image(file);
    let yuv420 = img.to_yuv420().unwrap();
    assert_eq!(H, yuv420.height);
    assert_eq!(W, yuv420.width);

    let reference = get_reference(img.reference).to_yuv420().unwrap();

    let Yuv420Diff { y, uv } = diff_yuv420(&yuv420, &reference);
    let ylen = yuv420.width as usize * yuv420.height as usize;
    if y.max_abs > 0 {
        print_hexdump_diff(
            "y:  ",
            "reference",
            &reference.data[..ylen],
            &format!("from {file}"),
            &yuv420.data[..ylen],
        );
    }
    if uv.max_abs > 0 {
        print_hexdump_diff(
            "uv: ",
            "reference",
            &reference.data[ylen..],
            &format!("from {file}"),
            &yuv420.data[ylen..],
        );
    }
    println!("y:  {y:#?}");
    println!("uv: {uv:#?}");
    assert!(y.psnr >= y_psnr, "y PSNR {} < {}", y.psnr, y_psnr);
    assert!(uv.psnr >= uv_psnr, "u PSNR {} < {}", uv.psnr, uv_psnr);
}

macro_rules! test {
    ($name:ident, $file:literal) => {
        test!(
            $name,
            $file,
            y_psnr = f64::INFINITY,
            uv_psnr = f64::INFINITY
        );
    };
    ($name:ident, $file:literal, y_psnr=$y_psnr:expr) => {
        test!($name, $file, y_psnr = $y_psnr, uv_psnr = f64::INFINITY);
    };
    ($name:ident, $file:literal, y_psnr=$y_psnr:expr, uv_psnr=$uv_psnr:expr) => {
        #[test]
        fn $name() {
            test_against_reference($file, $y_psnr, $uv_psnr);
        }
    };
}

// We can validate exact match for bayer-encoded images due to quirks in the image generation
// script (lossless decimation), and assumptions about the decoding process. This would not work if
// the decoder applied linear interpolation, for example.
test!(test_bayer8_bggr_yuv420, "test.bayer8-bggr.raw");
test!(test_bayer8_bggr_pad_yuv420, "test.bayer8-bggr.pad.raw");
test!(test_bayer8_gbrg_yuv420, "test.bayer8-gbrg.raw");
test!(test_bayer8_gbrg_pad_yuv420, "test.bayer8-gbrg.pad.raw");
test!(test_bayer8_grbg_yuv420, "test.bayer8-grbg.raw");
test!(test_bayer8_grbg_pad_yuv420, "test.bayer8-grbg.pad.raw");
test!(test_bayer8_rggb_yuv420, "test.bayer8-rggb.raw");
test!(test_bayer8_rggb_pad_yuv420, "test.bayer8-rggb.pad.raw");

test!(test_bgr8_yuv420, "test.bgr8.raw");
test!(test_bgr8_pad_yuv420, "test.bgr8.pad.raw");
test!(test_bgra8_yuv420, "test.bgra8.raw");
test!(test_bgra8_pad_yuv420, "test.bgra8.pad.raw");
test!(test_rgb8_yuv420, "test.rgb8.raw");
test!(test_rgb8_pad_yuv420, "test.rgb8.pad.raw");
test!(test_rgba8_yuv420, "test.rgba8.raw");
test!(test_rgba8_pad_yuv420, "test.rgba8.pad.raw");

test!(test_mono32fle_yuv420, "test.mono32fle.raw");
test!(test_mono32fle_pad_yuv420, "test.mono32fle.pad.raw");
test!(test_mono32fbe_yuv420, "test.mono32fbe.raw");
test!(test_mono32fbe_pad_yuv420, "test.mono32fbe.pad.raw");
test!(test_mono16le_yuv420, "test.mono16le.raw");
test!(test_mono16le_pad_yuv420, "test.mono16le.pad.raw");
test!(test_mono16be_yuv420, "test.mono16be.raw");
test!(test_mono16be_pad_yuv420, "test.mono16be.pad.raw");
// Allow minor (rounding) errors, since the mono8 source was downsampled from the mono32f that's
// being used as the monochrome reference image.
test!(test_mono8_yuv420, "test.mono8.raw", y_psnr = 53.);
test!(test_mono8_pad_yuv420, "test.mono8.pad.raw", y_psnr = 53.);

// Allow minor errors, since we're downsampling YUV422 to YUV420.
test!(
    test_uyvy_yuv420,
    "test.uyvy.raw",
    y_psnr = 54.,
    uv_psnr = 63.
);
test!(
    test_uyvy_pad_yuv420,
    "test.uyvy.pad.raw",
    y_psnr = 54.,
    uv_psnr = 63.
);
test!(
    test_yuyv_yuv420,
    "test.yuyv.raw",
    y_psnr = 54.,
    uv_psnr = 63.
);
test!(
    test_yuyv_pad_yuv420,
    "test.yuyv.pad.raw",
    y_psnr = 54.,
    uv_psnr = 63.
);

// Compressed formats. The low PSNR is entirely due to lossy compression algorithms that don't
// handle our test image very well.
#[cfg(feature = "img2yuv-png")]
test!(test_png_yuv420, "test.png");
#[cfg(feature = "img2yuv-jpeg")]
test!(test_jpeg_yuv420, "test.jpg", y_psnr = 34., uv_psnr = 25.);
#[cfg(feature = "img2yuv-webp")]
test!(test_webp_yuv420, "test.webp", y_psnr = 33., uv_psnr = 25.);

/// A discontiguous YUV 4:2:0 buffer with end-of-row padding in each plane.
#[derive(Clone)]
struct Yuv420Padded {
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
    y_stride: u32,
    u_stride: u32,
    v_stride: u32,
    width: u32,
    height: u32,
}
impl Yuv420Padded {
    fn new(width: u32, height: u32) -> Self {
        let y_stride = width + PAD;
        let u_stride = width / 2 + PAD + 1;
        let v_stride = width / 2 + PAD + 2;
        let y = vec![0; y_stride as usize * height as usize];
        let u = vec![0; u_stride as usize * height as usize / 2];
        let v = vec![0; v_stride as usize * height as usize / 2];
        Self {
            y,
            u,
            v,
            y_stride,
            u_stride,
            v_stride,
            width,
            height,
        }
    }

    /// Flattens the buffer into a contiguous Yuv420Vec, without end-of-row padding.
    fn into_yuv420_vec(self) -> Yuv420Vec {
        let mut data = vec![];
        let mut extend = |buf: &[u8], stride: u32, row_size: u32| {
            for row in buf.chunks_exact(stride as usize) {
                data.extend(&row[..row_size as usize]);
            }
        };
        extend(&self.y, self.y_stride, self.width);
        extend(&self.u, self.u_stride, self.width / 2);
        extend(&self.v, self.v_stride, self.width / 2);
        let yuv420_vec = Yuv420Vec {
            width: self.width,
            height: self.height,
            data,
        };
        yuv420_vec.validate().unwrap();
        yuv420_vec
    }
}
impl Yuv420Buffer for Yuv420Padded {
    fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn yuv(&self) -> (&[u8], &[u8], &[u8]) {
        (&self.y, &self.u, &self.v)
    }

    fn yuv_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
        (&mut self.y, &mut self.u, &mut self.v)
    }

    fn yuv_strides(&self) -> (u32, u32, u32) {
        (self.y_stride, self.u_stride, self.v_stride)
    }
}

#[test]
fn test_yuv420_buffer_validate_dimensions() {
    assert_matches!(
        Yuv420Vec::new(2, 4).validate_dimensions(3, 4),
        Err(Error::DimensionMismatch {
            expect: (3, 4),
            actual: (2, 4),
            ..
        })
    );
    assert_matches!(
        Yuv420Vec::new(2, 4).validate_dimensions(2, 3),
        Err(Error::DimensionMismatch {
            expect: (2, 3),
            actual: (2, 4),
            ..
        })
    );
}

#[test]
fn test_yuv420_buffer_validate() {
    let buf = Yuv420Padded::new(W, H);
    buf.validate().unwrap();

    macro_rules! stride_too_small{
        ($which:literal, $field:ident, $value:expr) => {
            let mut bad = buf.clone();
            bad.$field = $value;
            assert_matches!(
                bad.validate(),
                Err(Error::StrideTooSmall{
                    which,
                    stride,
                    ..
                }) if which == $which && stride == $value
            )
        }
    }
    stride_too_small!("y", y_stride, W - 1);
    stride_too_small!("u", u_stride, (W / 2) - 1);
    stride_too_small!("v", v_stride, (W / 2) - 1);

    macro_rules! buffer_too_small {
        ($which:literal, $field:ident, $size:expr) => {
            let mut bad = buf.clone();
            let size = $size as usize;
            bad.$field = vec![0; size];
            assert_matches!(
                bad.validate(),
                Err(Error::BufferTooSmall{
                    which,
                    actual,
                    ..
                }) if which == $which && actual == size
            )
        }
    }
    buffer_too_small!("y", y, buf.y_stride * H - 1);
    buffer_too_small!("u", u, buf.u_stride * H / 2 - 1);
    buffer_too_small!("v", v, buf.v_stride * H / 2 - 1);
}

/// Transcodes the image to YUV 4:2:0 using a discontiguous buffer with end-of-row padding.
/// Validates that the result matches what's written to contiguous buffer without padding.
///
/// We don't need to run this test against every possible encoding; we just need to coverage for
/// each of the main transcoding functions in `crate::raw::helpers`.
fn test_yuv420_padded(file: &str) {
    // Load reference.
    let img = get_image(file);
    let reference = img.to_yuv420().unwrap();
    let (expect_y, expect_u, expect_v) = reference.yuv();

    // Transcode to discontiguous padded buffers.
    let (width, height) = img.image.probe_dimensions().unwrap();
    let mut buf = Yuv420Padded::new(width, height);
    img.image.to_yuv420(&mut buf).unwrap();

    // Flatten back down to a contiguous buffer for comparison.
    let yuv420 = buf.into_yuv420_vec();
    let (y, u, v) = yuv420.yuv();

    assert_eq!(expect_y, y, "y planes differ");
    assert_eq!(expect_u, u, "u planes differ");
    assert_eq!(expect_v, v, "v planes differ");
}

#[test]
fn test_rgb8_yuv420_padded() {
    test_yuv420_padded("test.rgb8.raw");
}

#[test]
fn test_uyvy_yuv420_padded() {
    test_yuv420_padded("test.uyvy.raw");
}

#[test]
fn test_mono32fle_yuv420_padded() {
    test_yuv420_padded("test.mono32fle.raw");
}
