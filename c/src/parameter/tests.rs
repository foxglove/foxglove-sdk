use super::*;

#[test]
fn test_empty() {
    let mut p = std::ptr::null_mut();
    let err = unsafe { foxglove_parameter_create_empty(&raw mut p, "empty".into()) };
    assert_eq!(err, FoxgloveError::Ok);

    let p = unsafe { FoxgloveParameter::from_raw(p) };
    let p = p.into_native();
    assert_eq!(p, Parameter::empty("empty"));
}

#[test]
fn test_float() {
    let mut p = std::ptr::null_mut();
    let err = unsafe { foxglove_parameter_create_float64(&raw mut p, "float".into(), 1.23) };
    assert_eq!(err, FoxgloveError::Ok);

    let p = unsafe { FoxgloveParameter::from_raw(p) };
    let p = p.into_native();
    assert_eq!(p, Parameter::float64("float", 1.23));
}

#[test]
fn test_float_array() {
    let values = &[1.23, 4.56];
    let mut p = std::ptr::null_mut();
    let err = unsafe {
        foxglove_parameter_create_float64_array(
            &raw mut p,
            "float array".into(),
            values.as_ptr(),
            values.len(),
        )
    };
    assert_eq!(err, FoxgloveError::Ok);

    let p = unsafe { FoxgloveParameter::from_raw(p) };
    let p = p.into_native();
    assert_eq!(p, Parameter::float64_array("float array", vec![1.23, 4.56]));
}

#[test]
fn test_string() {
    let mut p = std::ptr::null_mut();
    let err =
        unsafe { foxglove_parameter_create_string(&raw mut p, "string".into(), "data".into()) };
    assert_eq!(err, FoxgloveError::Ok);

    let p = unsafe { FoxgloveParameter::from_raw(p) };
    let p = p.into_native();
    assert_eq!(p, Parameter::string("string", "data"));
}

#[test]
fn test_byte_array() {
    let mut p = std::ptr::null_mut();
    let err = unsafe {
        foxglove_parameter_create_byte_array(
            &raw mut p,
            "string".into(),
            FoxgloveBytes::from_slice(b"data"),
        )
    };
    assert_eq!(err, FoxgloveError::Ok);

    let p = unsafe { FoxgloveParameter::from_raw(p) };
    let p = p.into_native();
    assert_eq!(p, Parameter::byte_array("string", b"data"));
}

#[test]
fn test_bool() {
    let mut p = std::ptr::null_mut();
    let err = unsafe { foxglove_parameter_create_boolean(&raw mut p, "bool".into(), true) };
    assert_eq!(err, FoxgloveError::Ok);

    let p = unsafe { FoxgloveParameter::from_raw(p) };
    let p = p.into_native();
    assert_eq!(p, Parameter::bool("bool", true));
}

macro_rules! make_value {
    ($ctor:ident, $value:expr) => {{
        let mut value_ptr = std::ptr::null_mut();
        #[allow(unused_unsafe)]
        let err = unsafe { $ctor(&mut value_ptr, $value) };
        assert_eq!(err, FoxgloveError::Ok);
        value_ptr
    }};
}

macro_rules! array_insert {
    ($array_ptr:ident, $value_ptr:expr) => {
        let err =
            unsafe { foxglove_parameter_value_array_push(Some(&mut *$array_ptr), $value_ptr) };
        assert_eq!(err, FoxgloveError::Ok);
    };
}

macro_rules! dict_insert {
    ($dict:ident, $key:literal, $value_ptr:expr) => {
        let err = unsafe {
            foxglove_parameter_value_dict_insert(Some(&mut *$dict), $key.into(), $value_ptr)
        };
        assert_eq!(err, FoxgloveError::Ok);
    };
}

fn make_dict_param() -> *mut FoxgloveParameter {
    let inner = foxglove_parameter_value_dict_create(2);
    dict_insert!(
        inner,
        "string",
        make_value!(foxglove_parameter_value_create_string, "xyzzy".into())
    );
    let array_ptr = foxglove_parameter_value_array_create(2);
    array_insert!(
        array_ptr,
        foxglove_parameter_value_create_float64(std::f64::consts::E)
    );
    array_insert!(
        array_ptr,
        foxglove_parameter_value_create_float64(std::f64::consts::PI)
    );
    dict_insert!(
        inner,
        "f64[]",
        foxglove_parameter_value_create_array(array_ptr)
    );

    let outer = foxglove_parameter_value_dict_create(3);
    dict_insert!(
        outer,
        "bool",
        foxglove_parameter_value_create_boolean(false)
    );
    dict_insert!(
        outer,
        "float64",
        foxglove_parameter_value_create_float64(1.23)
    );
    dict_insert!(outer, "nested", foxglove_parameter_value_create_dict(inner));

    let mut param = std::ptr::null_mut();
    let err = unsafe { foxglove_parameter_create_dict(&raw mut param, "outer".into(), outer) };
    assert_eq!(err, FoxgloveError::Ok);
    param
}

fn make_dict_native() -> Parameter {
    Parameter::dict(
        "outer",
        maplit::btreemap! {
            "bool".into() => ParameterValue::Bool(false),
            "nested".into() => ParameterValue::Dict(
                maplit::btreemap! {
                    "string".into() => ParameterValue::String("xyzzy".into()),
                    "f64[]".into() => ParameterValue::Array(vec![
                        ParameterValue::Float64(std::f64::consts::E),
                        ParameterValue::Float64(std::f64::consts::PI),
                    ]),
                }
            ),
            "float64".into() => ParameterValue::Float64(1.23),
        },
    )
}

#[test]
fn test_dict() {
    let raw = make_dict_param();
    let param = unsafe { FoxgloveParameter::from_raw(raw) };
    assert_eq!(param.into_native(), make_dict_native());
}

#[test]
fn test_clone() {
    let src = make_dict_param();
    let dst = unsafe { foxglove_parameter_clone(Some(&*src)) };
    unsafe { foxglove_parameter_free(src) };
    let param = unsafe { FoxgloveParameter::from_raw(dst) };
    assert_eq!(param.into_native(), make_dict_native());
}
