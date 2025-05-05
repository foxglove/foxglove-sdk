#pragma once

#include <cstdint>

#include "expected.hpp"

namespace foxglove {

enum class FoxgloveError : uint8_t {
  Ok,
  Unspecified,
  ValueError,
  Utf8Error,
  SinkClosed,
  SchemaRequired,
  MessageEncodingRequired,
  ServerAlreadyStarted,
  Bind,
  DuplicateService,
  MissingRequestEncoding,
  ServicesNotSupported,
  ConnectionGraphNotSupported,
  IoError,
  McapError,
  BufferTooShort,
  Base64DecodeError
};

template<typename T>
using FoxgloveResult = expected<T, FoxgloveError>;

const char* strerror(FoxgloveError error);

}  // namespace foxglove
