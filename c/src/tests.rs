use std::pin::pin;

use crate::{
    CircleAnnotation, FoxglovePointsAnnotationType, FoxgloveString, FoxgloveTimestamp,
    ImageAnnotations, Point2, PointsAnnotation, TextAnnotation,
    arena::{Arena, BorrowToNative},
    generated_types::{Color, Point3, Pose, Quaternion, TriangleListPrimitive, Vector3},
};
use foxglove::messages::TriangleListPrimitive as NativeTriangleListPrimitive;
use std::ffi::c_char;

#[test]
fn test_foxglove_string_as_utf8_str() {
    let string = FoxgloveString {
        data: c"test".as_ptr(),
        len: 4,
    };
    let utf8_str = unsafe { string.as_utf8_str() };
    assert_eq!(utf8_str, Ok("test"));

    let string = FoxgloveString {
        data: c"💖".as_ptr(),
        len: 4,
    };
    let utf8_str = unsafe { string.as_utf8_str() };
    assert_eq!(utf8_str, Ok("💖"));
}

#[test]
fn test_triangle_list_primitive_borrow_to_native() {
    let reference = NativeTriangleListPrimitive {
        pose: Some(foxglove::messages::Pose {
            position: Some(foxglove::messages::Vector3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            }),
            orientation: Some(foxglove::messages::Quaternion {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0,
            }),
        }),
        points: vec![
            foxglove::messages::Point3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            foxglove::messages::Point3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            foxglove::messages::Point3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
        ],
        color: Some(foxglove::messages::Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        }),
        colors: vec![
            foxglove::messages::Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            foxglove::messages::Color {
                r: 0.0,
                g: 1.0,
                b: 1.0,
                a: 0.0,
            },
        ],
        indices: vec![0, 1, 2],
    };

    let pose = Pose {
        position: &Vector3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        orientation: &Quaternion {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        },
    };

    let points = [
        Point3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        Point3 {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        },
        Point3 {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        },
    ];

    let color = Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    let colors = [
        Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
        Color {
            r: 0.0,
            g: 1.0,
            b: 1.0,
            a: 0.0,
        },
    ];

    let indices = [0, 1, 2];
    let c_type = TriangleListPrimitive {
        pose: &raw const pose,
        points: points.as_ptr(),
        points_count: points.len(),
        color: &raw const color,
        colors: colors.as_ptr(),
        colors_count: colors.len(),
        indices: indices.as_ptr(),
        indices_count: indices.len(),
    };

    let mut arena = pin!(Arena::new());
    let arena_pin = arena.as_mut();
    let borrowed = unsafe { c_type.borrow_to_native(arena_pin).unwrap() };

    assert_eq!(*borrowed, reference);
}

#[test]
fn test_image_annotations_borrow_to_native() {
    // Create reference native ImageAnnotations struct
    let reference = foxglove::messages::ImageAnnotations {
        circles: vec![foxglove::messages::CircleAnnotation {
            timestamp: Some(foxglove::messages::Timestamp::new(1000000000, 500000000)),
            position: Some(foxglove::messages::Point2 { x: 10.0, y: 20.0 }),
            diameter: 15.0,
            thickness: 2.0,
            fill_color: Some(foxglove::messages::Color {
                r: 1.0,
                g: 0.5,
                b: 0.3,
                a: 0.8,
            }),
            outline_color: Some(foxglove::messages::Color {
                r: 0.1,
                g: 0.2,
                b: 0.9,
                a: 1.0,
            }),
        }],
        points: vec![foxglove::messages::PointsAnnotation {
            timestamp: Some(foxglove::messages::Timestamp::new(1000000000, 500000000)),
            r#type: foxglove::messages::points_annotation::Type::LineStrip as i32,
            points: vec![
                foxglove::messages::Point2 { x: 5.0, y: 10.0 },
                foxglove::messages::Point2 { x: 15.0, y: 25.0 },
                foxglove::messages::Point2 { x: 30.0, y: 15.0 },
            ],
            outline_color: Some(foxglove::messages::Color {
                r: 0.8,
                g: 0.2,
                b: 0.3,
                a: 1.0,
            }),
            outline_colors: vec![foxglove::messages::Color {
                r: 0.9,
                g: 0.1,
                b: 0.2,
                a: 1.0,
            }],
            fill_color: Some(foxglove::messages::Color {
                r: 0.2,
                g: 0.8,
                b: 0.3,
                a: 0.5,
            }),
            thickness: 3.0,
        }],
        texts: vec![foxglove::messages::TextAnnotation {
            timestamp: Some(foxglove::messages::Timestamp::new(1000000000, 500000000)),
            position: Some(foxglove::messages::Point2 { x: 50.0, y: 60.0 }),
            text: "Sample text".to_string(),
            font_size: 14.0,
            text_color: Some(foxglove::messages::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }),
            background_color: Some(foxglove::messages::Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.7,
            }),
        }],
        metadata: vec![],
    };

    // Create the timestamp value
    let timestamp = FoxgloveTimestamp {
        sec: 1000000000,
        nsec: 500000000,
    };

    // Create circle annotation
    let circle_position = Point2 { x: 10.0, y: 20.0 };

    let circle_fill_color = Color {
        r: 1.0,
        g: 0.5,
        b: 0.3,
        a: 0.8,
    };

    let circle_outline_color = Color {
        r: 0.1,
        g: 0.2,
        b: 0.9,
        a: 1.0,
    };

    let circle = CircleAnnotation {
        timestamp: &raw const timestamp,
        position: &raw const circle_position,
        diameter: 15.0,
        thickness: 2.0,
        fill_color: &raw const circle_fill_color,
        outline_color: &raw const circle_outline_color,
    };

    // Create points annotation
    let points_positions = [
        Point2 { x: 5.0, y: 10.0 },
        Point2 { x: 15.0, y: 25.0 },
        Point2 { x: 30.0, y: 15.0 },
    ];

    let points_outline_color = Color {
        r: 0.8,
        g: 0.2,
        b: 0.3,
        a: 1.0,
    };

    let points_outline_colors = [Color {
        r: 0.9,
        g: 0.1,
        b: 0.2,
        a: 1.0,
    }];

    let points_fill_color = Color {
        r: 0.2,
        g: 0.8,
        b: 0.3,
        a: 0.5,
    };

    let points = PointsAnnotation {
        timestamp: &raw const timestamp,
        r#type: FoxglovePointsAnnotationType::LineStrip,
        points: points_positions.as_ptr(),
        points_count: points_positions.len(),
        outline_color: &raw const points_outline_color,
        outline_colors: points_outline_colors.as_ptr(),
        outline_colors_count: points_outline_colors.len(),
        fill_color: &raw const points_fill_color,
        thickness: 3.0,
    };

    // Create text annotation
    let text_position = Point2 { x: 50.0, y: 60.0 };

    let text_color = Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    let background_color = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 0.7,
    };

    // Convert "Sample text" to FoxgloveString
    let sample_text = "Sample text";
    let text = FoxgloveString {
        data: sample_text.as_ptr() as *const c_char,
        len: sample_text.len(),
    };

    let text_annotation = TextAnnotation {
        timestamp: &raw const timestamp,
        position: &raw const text_position,
        text,
        font_size: 14.0,
        text_color: &raw const text_color,
        background_color: &raw const background_color,
    };

    // Create ImageAnnotations struct
    let circles = [circle];
    let points_arr = [points];
    let texts = [text_annotation];

    let c_type = ImageAnnotations {
        circles: circles.as_ptr(),
        circles_count: circles.len(),
        points: points_arr.as_ptr(),
        points_count: points_arr.len(),
        texts: texts.as_ptr(),
        texts_count: texts.len(),
        metadata: std::ptr::null(),
        metadata_count: 0,
    };

    let mut arena = pin!(Arena::new());
    let arena_pin = arena.as_mut();
    let borrowed = unsafe { c_type.borrow_to_native(arena_pin).unwrap() };

    assert_eq!(*borrowed, reference);
}
