from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from typing import ClassVar as _ClassVar, Optional as _Optional

DESCRIPTOR: _descriptor.FileDescriptor

class Apple(_message.Message):
    __slots__ = ("color", "diameter")
    COLOR_FIELD_NUMBER: _ClassVar[int]
    DIAMETER_FIELD_NUMBER: _ClassVar[int]
    color: str
    diameter: int
    def __init__(self, color: _Optional[str] = ..., diameter: _Optional[int] = ...) -> None: ...
