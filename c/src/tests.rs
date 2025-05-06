use std::pin::pin;

use crate::{
    arena::{Arena, BorrowToNative},
    generated_types::{Color, Point3, Pose, Quaternion, TriangleListPrimitive, Vector3},
    FoxgloveString,
};
use foxglove::schemas::TriangleListPrimitive as NativeTriangleListPrimitive;

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
        pose: Some(foxglove::schemas::Pose {
            position: Some(foxglove::schemas::Vector3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            }),
            orientation: Some(foxglove::schemas::Quaternion {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0,
            }),
        }),
        points: vec![
            foxglove::schemas::Point3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            foxglove::schemas::Point3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            foxglove::schemas::Point3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
        ],
        color: Some(foxglove::schemas::Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        }),
        colors: vec![
            foxglove::schemas::Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            foxglove::schemas::Color {
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
        pose: &pose,
        points: points.as_ptr(),
        points_count: points.len(),
        color: &color,
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
