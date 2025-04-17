#pragma once

#include <exception>
#include <memory>

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
  DuplicateChannel,
  DuplicateService,
  MissingRequestEncoding,
  ServicesNotSupported,
  ConnectionGraphNotSupported,
  IoError,
  McapError
};

template<typename T>
using FoxgloveResult = expected<T, FoxgloveError>;

const char* strerror(FoxgloveError error);

}  // namespace foxglove
