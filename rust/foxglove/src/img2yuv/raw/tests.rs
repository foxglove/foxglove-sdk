use assert_matches::assert_matches;

use super::{BayerCfa, BayerPixel, Endian, Error, RawImage, RawImageEncoding};
use crate::img2yuv::{Yuv420Buffer, Yuv420Vec};

#[test]
fn test_validate_zero_sized() {
    for (width, height) in [(0, 4), (4, 0)] {
        let err = RawImage {
            encoding: RawImageEncoding::Mono8,
            width,
            height,
            stride: 4,
            data: vec![0; 16].into(),
        }
        .validate_dimensions()
        .unwrap_err();
        assert_matches!(err, Error::ZeroSized);
    }
}

#[test]
fn test_validate_dimensions_stride_too_small() {
    let err = RawImage {
        encoding: RawImageEncoding::Rgb8,
        width: 3,
        height: 4,
        stride: 8,
        data: b"".into(),
    }
    .validate_dimensions()
    .unwrap_err();
    assert_matches!(
        err,
        Error::StrideTooSmall {
            stride: 8,
            row_size: 9,
            ..
        }
    );
}

#[test]
fn test_validate_dimensions_buffer_too_small() {
    let err = RawImage {
        encoding: RawImageEncoding::Rgb8,
        width: 3,
        height: 2,
        stride: 12,
        data: vec![0; 23].into(),
    }
    .validate_dimensions()
    .unwrap_err();
    assert_matches!(
        err,
        Error::BufferTooSmall {
            actual: 23,
            expect: 24,
            ..
        }
    );
}

#[test]
fn test_validate_dimensions_bayer_dimensions_even() {
    for (width, height) in [(3, 4), (4, 3)] {
        let err = RawImage {
            encoding: RawImageEncoding::Bayer8(BayerCfa::Rggb),
            width,
            height,
            stride: 12,
            data: vec![0; 12 * height as usize].into(),
        }
        .validate_dimensions()
        .unwrap_err();
        assert_matches!(err, Error::BayerDimensionsMustBeEven{ width: w, height: h } if w == width && h == height);
    }
}

#[test]
fn test_validate_dimensions_yuv422_must_be_even() {
    for encoding in [RawImageEncoding::Uyvy, RawImageEncoding::Yuyv] {
        let err = RawImage {
            encoding,
            width: 3,
            height: 2,
            stride: 8,
            data: vec![0; 16].into(),
        }
        .validate_dimensions()
        .unwrap_err();
        assert_matches!(err, Error::Yuv422WidthMustBeEven { width: 3 });
    }
}

#[test]
fn test_validate_dimensions_nv12_must_be_even() {
    for (width, height) in [(3, 4), (4, 3)] {
        // NV12 buffer: stride * height * 3/2 (rounded up for odd cases)
        let stride = width + 2;
        let buf_size = stride as usize * height as usize * 2;
        let err = RawImage {
            encoding: RawImageEncoding::Nv12,
            width,
            height,
            stride,
            data: vec![0; buf_size].into(),
        }
        .validate_dimensions()
        .unwrap_err();
        assert_matches!(err, Error::Nv12DimensionsMustBeEven{ width: w, height: h } if w == width && h == height);
    }
}

#[test]
fn test_validate_dimensions_nv12_buffer_size() {
    // NV12 needs stride * height * 3/2 bytes
    let width = 4;
    let height = 4;
    let stride = 4;
    let expected_size = stride as usize * height as usize * 3 / 2; // 24

    // Buffer that's one byte too small.
    let err = RawImage {
        encoding: RawImageEncoding::Nv12,
        width,
        height,
        stride,
        data: vec![0; expected_size - 1].into(),
    }
    .validate_dimensions()
    .unwrap_err();
    assert_matches!(
        err,
        Error::BufferTooSmall {
            actual,
            expect,
            ..
        } if actual == expected_size - 1 && expect == expected_size
    );

    // Exact size should pass.
    RawImage {
        encoding: RawImageEncoding::Nv12,
        width,
        height,
        stride,
        data: vec![0; expected_size].into(),
    }
    .validate_dimensions()
    .unwrap();
}

#[test]
fn test_nv12_to_yuv420() {
    // 4x4 NV12 image
    let width: u32 = 4;
    let height: u32 = 4;
    let stride = width;

    // Y plane: 4x4 = 16 bytes
    #[rustfmt::skip]
    let y_data: Vec<u8> = vec![
        16, 35, 16, 35,
        16, 35, 16, 35,
        82, 82, 82, 82,
        82, 82, 82, 82,
    ];
    // UV plane: 4x2 = 8 bytes (interleaved U,V pairs)
    #[rustfmt::skip]
    let uv_data: Vec<u8> = vec![
        128, 128, 200, 50,
        128, 128, 200, 50,
    ];

    let mut data = Vec::new();
    data.extend_from_slice(&y_data);
    data.extend_from_slice(&uv_data);

    let img = RawImage {
        encoding: RawImageEncoding::Nv12,
        width,
        height,
        stride,
        data: data.into(),
    };

    let mut buf = Yuv420Vec::new(width, height);
    img.to_yuv420(&mut buf).unwrap();

    let (y, u, v) = buf.yuv();
    assert_eq!(y, &y_data);
    assert_eq!(u, &[128, 200, 128, 200]);
    assert_eq!(v, &[128, 50, 128, 50]);
}

#[test]
fn test_pixels() {
    let data = vec![
        0, 1, 2, 3, 4, 5, 0xff, 0xff, // row 1
        6, 7, 8, 9, 10, 11, 0xff, 0xff, // row 2
    ];
    let img = RawImage {
        encoding: RawImageEncoding::Rgb8,
        width: 2,
        height: 2,
        stride: 8,
        data: data.as_slice().into(),
    };
    let pixels: Vec<_> = img.pixels().collect();
    assert_eq!(
        pixels,
        vec![&[0, 1, 2], &[3, 4, 5], &[6, 7, 8], &[9, 10, 11]]
    );

    let img = RawImage {
        encoding: RawImageEncoding::Mono16(Endian::Little),
        width: 3,
        height: 2,
        stride: 8,
        data: data.as_slice().into(),
    };
    let pixels: Vec<_> = img.pixels().collect();
    assert_eq!(
        pixels,
        vec![&[0, 1], &[2, 3], &[4, 5], &[6, 7], &[8, 9], &[10, 11]]
    );

    let img = RawImage {
        encoding: RawImageEncoding::Mono8,
        width: 6,
        height: 2,
        stride: 8,
        data: data.as_slice().into(),
    };
    let pixels: Vec<_> = img.pixels().collect();
    assert_eq!(
        pixels,
        vec![
            &[0],
            &[1],
            &[2],
            &[3],
            &[4],
            &[5],
            &[6],
            &[7],
            &[8],
            &[9],
            &[10],
            &[11]
        ]
    );
}

#[test]
fn test_bayer8_pixels() {
    let data = vec![
        0, 1, 4, 5, 0xff, 0xff, // row 1
        2, 3, 6, 7, 0xff, 0xff, // row 2
        8, 9, 12, 13, 0xff, 0xff, // row 3
        10, 11, 14, 15, 0xff, 0xff, // row 4
    ];
    let img = RawImage {
        encoding: RawImageEncoding::Bayer8(BayerCfa::Rggb),
        width: 4,
        height: 4,
        stride: 6,
        data: data.as_slice().into(),
    };
    let pixels: Vec<_> = img.bayer8_pixels().unwrap().collect();
    assert_eq!(
        pixels,
        vec![
            BayerPixel {
                r: 0,
                g0: 1,
                g1: 2,
                b: 3,
                row: 0,
                col: 0,
            },
            BayerPixel {
                r: 4,
                g0: 5,
                g1: 6,
                b: 7,
                row: 0,
                col: 2,
            },
            BayerPixel {
                r: 8,
                g0: 9,
                g1: 10,
                b: 11,
                row: 2,
                col: 0,
            },
            BayerPixel {
                r: 12,
                g0: 13,
                g1: 14,
                b: 15,
                row: 2,
                col: 2,
            },
        ]
    );

    // Other CFA values.
    for (cfa, (r, g0, g1, b)) in [
        (BayerCfa::Gbrg, (2, 0, 3, 1)),
        (BayerCfa::Grbg, (1, 0, 3, 2)),
        (BayerCfa::Bggr, (3, 1, 2, 0)),
    ] {
        let img = RawImage {
            encoding: RawImageEncoding::Bayer8(cfa),
            width: 4,
            height: 4,
            stride: 6,
            data: data.as_slice().into(),
        };
        let first_pixel = img.bayer8_pixels().unwrap().next().unwrap();

        assert_eq!(
            first_pixel,
            BayerPixel {
                r,
                g0,
                g1,
                b,
                row: 0,
                col: 0
            },
            "{cfa:?}",
        );
    }
}
