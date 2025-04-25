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
    assert p.value == ParameterValue.Number(1.234)
    assert p.get_value() == 1.234


def test_int() -> None:
    p = Parameter("int", value=1)
    assert p.name == "int"
    assert p.type == ParameterType.Float64
    assert p.value == ParameterValue.Number(1)
    assert type(p.get_value()) is float
    assert p.get_value() == 1


def test_float_array() -> None:
    v: AnyNativeParameterValue = [1, 2, 3]
    p = Parameter("float_array", value=v)
    assert p.name == "float_array"
    assert p.type == ParameterType.Float64Array
    assert p.value == ParameterValue.Array(
        [
            ParameterValue.Number(1),
            ParameterValue.Number(2),
            ParameterValue.Number(3),
        ]
    )
    assert p.get_value() == v


def test_heterogeneous_array() -> None:
    v: AnyNativeParameterValue = ["a", 2, False]
    p = Parameter("heterogeneous_array", value=v)
    assert p.name == "heterogeneous_array"
    assert p.type is None
    assert p.value == ParameterValue.Array(
        [
            ParameterValue.String("a"),
            ParameterValue.Number(2),
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
            "b": ParameterValue.Number(2),
            "c": ParameterValue.String("C"),
            "d": ParameterValue.Dict(
                {
                    "inner": ParameterValue.Array(
                        [
                            ParameterValue.Number(1),
                            ParameterValue.Number(2),
                            ParameterValue.Number(3),
                        ]
                    )
                }
            ),
        }
    )
    assert p.get_value() == v
