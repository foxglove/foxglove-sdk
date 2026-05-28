import pytest
from foxglove.websocket import (
    AnyNativeParameterValue,
    Parameter,
    ParameterType,
    ParameterValue,
)


def test_empty() -> None:
    p = Parameter("empty")
    assert p.name == "empty"
    assert p.type is None
    assert p.value is None
    assert p.get_value() is None


def test_float() -> None:
    p = Parameter("float", value=1.234)
    assert p.name == "float"
    assert p.type == ParameterType.Float64
    assert p.value == ParameterValue.Float64(1.234)
    assert p.get_value() == 1.234


def test_int() -> None:
    p = Parameter("int", value=1)
    assert p.name == "int"
    assert p.type is None
    assert p.value == ParameterValue.Integer(1)
    assert type(p.get_value()) is int
    assert p.get_value() == 1


def test_float_array() -> None:
    v: AnyNativeParameterValue = [1.0, 2.0, 3.0]
    p = Parameter("float_array", value=v)
    assert p.name == "float_array"
    assert p.type == ParameterType.Float64Array
    assert p.value == ParameterValue.Array(
        [
            ParameterValue.Float64(1.0),
            ParameterValue.Float64(2.0),
            ParameterValue.Float64(3.0),
        ]
    )
    assert p.get_value() == v


def test_int_array() -> None:
    v: AnyNativeParameterValue = [1, 2, 3]
    p = Parameter("int_array", value=v)
    assert p.name == "int_array"
    assert p.type is None
    assert p.value == ParameterValue.Array(
        [
            ParameterValue.Integer(1),
            ParameterValue.Integer(2),
            ParameterValue.Integer(3),
        ]
    )
    assert p.get_value() == v


def test_parameter_value_integer() -> None:
    p = Parameter("integer_param", value=ParameterValue.Integer(42))
    assert p.name == "integer_param"
    assert p.type is None
    assert p.value == ParameterValue.Integer(42)
    assert type(p.get_value()) is int
    assert p.get_value() == 42


def test_heterogeneous_array() -> None:
    v: AnyNativeParameterValue = ["a", 2, False]
    p = Parameter("heterogeneous_array", value=v)
    assert p.name == "heterogeneous_array"
    assert p.type is None
    assert p.value == ParameterValue.Array(
        [
            ParameterValue.String("a"),
            ParameterValue.Integer(2),
            ParameterValue.Bool(False),
        ]
    )
    assert p.get_value() == v


def test_string() -> None:
    p = Parameter("string", value="hello")
    assert p.name == "string"
    assert p.type is None
    assert p.value == ParameterValue.String("hello")
    assert p.get_value() == "hello"


def test_bytes() -> None:
    p = Parameter("bytes", value=b"hello")
    assert p.name == "bytes"
    assert p.type == ParameterType.ByteArray
    assert p.value == ParameterValue.String("aGVsbG8=")
    assert p.get_value() == b"hello"


def test_dict() -> None:
    v: AnyNativeParameterValue = {
        "a": True,
        "b": 2,
        "c": "C",
        "d": {"inner": [1, 2, 3]},
    }
    p = Parameter(
        "dict",
        value=v,
    )
    assert p.name == "dict"
    assert p.type is None
    assert p.value == ParameterValue.Dict(
        {
            "a": ParameterValue.Bool(True),
            "b": ParameterValue.Integer(2),
            "c": ParameterValue.String("C"),
            "d": ParameterValue.Dict(
                {
                    "inner": ParameterValue.Array(
                        [
                            ParameterValue.Integer(1),
                            ParameterValue.Integer(2),
                            ParameterValue.Integer(3),
                        ]
                    )
                }
            ),
        }
    )
    assert p.get_value() == v


def test_explicit() -> None:
    # Derive type from value
    p = Parameter("float", value=ParameterValue.Float64(1))
    assert p.type == ParameterType.Float64
    assert p.get_value() == 1

    # Override derived type.
    p = Parameter(
        "bad float array",
        value=ParameterValue.Float64(1),
        type=ParameterType.Float64Array,
    )
    assert p.type == ParameterType.Float64Array
    assert p.get_value() == 1

    # Override derived type in a different way.
    p = Parameter(
        "bad float",
        value=ParameterValue.String("1"),
        type=ParameterType.Float64,
    )
    assert p.type == ParameterType.Float64
    assert p.get_value() == "1"

    # Override derived type with None.
    p = Parameter("underspecified float", value=ParameterValue.Float64(1), type=None)
    assert p.type is None
    assert p.get_value() == 1


def test_base64_decode_error() -> None:
    p = Parameter(
        "bad bytes",
        value=ParameterValue.String("!!!"),
        type=ParameterType.ByteArray,
    )
    assert p.type == ParameterType.ByteArray
    assert p.value == ParameterValue.String("!!!")
    with pytest.raises(ValueError, match=r"Failed to decode base64"):
        p.get_value()
