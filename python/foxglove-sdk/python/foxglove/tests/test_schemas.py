from foxglove.schemas import Log, LogLevel, Timestamp

""" Asserts that foxglove schemas can be encoded as protobuf. """


def test_can_encode() -> None:
    msg = Log(
        timestamp=Timestamp(5, 10),
        level=LogLevel.Error,
        message="hello",
        name="logger",
        file="file",
        line=123,
    )
    encoded = msg.encode()
    assert isinstance(encoded, bytes)
    assert len(encoded) == 34
